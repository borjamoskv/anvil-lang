// ============================================================
// ANVIL PARSER — Pest PEG → AST
// ============================================================

use pest::Parser;
use pest_derive::Parser;
use crate::ast::*;

#[derive(Parser)]
#[grammar = "grammar.pest"]
pub struct AnvilParser;

/// Parse an Anvil source file into an AST Program
pub fn parse_program(source: &str) -> Result<Program, String> {
    let pairs = AnvilParser::parse(Rule::program, source)
        .map_err(|e| format!("Parse error:\n{}", e))?;

    let mut items = Vec::new();
    for pair in pairs {
        if pair.as_rule() == Rule::program {
            for inner in pair.into_inner() {
                match inner.as_rule() {
                    Rule::fn_def => items.push(Item::Function(parse_fn_def(inner)?)),
                    Rule::struct_def => items.push(Item::Struct(parse_struct_def(inner)?)),
                    Rule::const_def => items.push(Item::Const(parse_const_def(inner)?)),
                    Rule::contract_def => items.push(Item::Contract(parse_contract_def(inner)?)),
                    Rule::EOI => {},
                    _ => {},
                }
            }
        }
    }

    Ok(Program { items })
}

fn parse_fn_def(pair: pest::iterators::Pair<Rule>) -> Result<FnDef, String> {
    let mut name = String::new();
    let mut is_pub = false;
    let mut params = Vec::new();
    let mut return_type = None;
    let mut invariants = Vec::new();
    let mut body = Block { stmts: vec![], expr: None };

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::visibility => is_pub = true,
            Rule::ident => name = inner.as_str().to_string(),
            Rule::param_list => params = parse_param_list(inner)?,
            Rule::type_expr => return_type = Some(parse_type(inner)?),
            Rule::where_clause => invariants = parse_where_clause(inner)?,
            Rule::block => body = parse_block(inner)?,
            _ => {},
        }
    }

    Ok(FnDef { name, is_pub, params, return_type, invariants, body, span: None })
}

fn parse_param_list(pair: pest::iterators::Pair<Rule>) -> Result<Vec<Param>, String> {
    let mut params = Vec::new();
    for inner in pair.into_inner() {
        if inner.as_rule() == Rule::param {
            params.push(parse_param(inner)?);
        }
    }
    Ok(params)
}

fn parse_param(pair: pest::iterators::Pair<Rule>) -> Result<Param, String> {
    let mut name = String::new();
    let mut ty = Type::Unit;
    let mut is_mut = false;

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::ident => {
                let s = inner.as_str();
                if s == "mut" { is_mut = true; } else { name = s.to_string(); }
            },
            Rule::type_expr => ty = parse_type(inner)?,
            _ => {},
        }
    }
    Ok(Param { name, ty, is_mut })
}

fn parse_type(pair: pest::iterators::Pair<Rule>) -> Result<Type, String> {
    let inner = pair.into_inner().next().unwrap();
    match inner.as_rule() {
        Rule::base_type => {
            let s = inner.as_str();
            Ok(match s {
                "u8" => Type::U8, "u16" => Type::U16, "u32" => Type::U32,
                "u64" => Type::U64, "u128" => Type::U128, "u256" => Type::U256,
                "i8" => Type::I8, "i16" => Type::I16, "i32" => Type::I32,
                "i64" => Type::I64, "i128" => Type::I128,
                "bool" => Type::Bool, "Address" => Type::Address,
                "String" => Type::String, "Unit" => Type::Unit,
                other => Type::Named(other.to_string()),
            })
        },
        Rule::array_type => {
            let inner_ty = parse_type(inner.into_inner().next().unwrap())?;
            Ok(Type::Array(Box::new(inner_ty)))
        },
        _ => Ok(Type::Unit),
    }
}

// THE CORE: Parse where clause into invariants
fn parse_where_clause(pair: pest::iterators::Pair<Rule>) -> Result<Vec<Invariant>, String> {
    let mut invariants = Vec::new();
    for inner in pair.into_inner() {
        if inner.as_rule() == Rule::invariant {
            invariants.push(parse_invariant(inner)?);
        }
    }
    Ok(invariants)
}

fn parse_invariant(pair: pest::iterators::Pair<Rule>) -> Result<Invariant, String> {
    let inner = pair.into_inner().next().unwrap();
    let expr = parse_invariant_expr(inner)?;
    Ok(Invariant { expr, span: None })
}

fn parse_invariant_expr(pair: pest::iterators::Pair<Rule>) -> Result<InvariantExpr, String> {
    match pair.as_rule() {
        Rule::logical_or_expr => {
            let mut inners: Vec<_> = pair.into_inner()
                .filter(|p| p.as_rule() != Rule::or_inv_op)
                .collect();
            if inners.len() == 1 {
                return parse_invariant_expr(inners.remove(0));
            }
            let mut left = parse_invariant_expr(inners.remove(0))?;
            for inner in inners {
                let right = parse_invariant_expr(inner)?;
                left = InvariantExpr::Or(Box::new(left), Box::new(right));
            }
            Ok(left)
        },
        Rule::logical_and_expr => {
            let mut inners: Vec<_> = pair.into_inner()
                .filter(|p| p.as_rule() != Rule::and_inv_op)
                .collect();
            if inners.len() == 1 {
                return parse_invariant_expr(inners.remove(0));
            }
            let mut left = parse_invariant_expr(inners.remove(0))?;
            for inner in inners {
                let right = parse_invariant_expr(inner)?;
                left = InvariantExpr::And(Box::new(left), Box::new(right));
            }
            Ok(left)
        },
        Rule::comparison_expr => {
            let mut inners: Vec<_> = pair.into_inner().collect();
            if inners.len() == 1 {
                // Single term — wrap as comparison with True
                let term = parse_inv_additive(inners.remove(0))?;
                return Ok(InvariantExpr::Comparison {
                    left: Box::new(term), op: CmpOp::Neq,
                    right: Box::new(InvTerm::Literal(0)),
                });
            }
            // left op right
            let left = parse_inv_additive(inners.remove(0))?;
            let op = parse_cmp_op(inners.remove(0))?;
            let right = parse_inv_additive(inners.remove(0))?;
            Ok(InvariantExpr::Comparison { left: Box::new(left), op, right: Box::new(right) })
        },
        Rule::quantifier => {
            let mut inners = pair.into_inner();
            let q_type = inners.next().unwrap().as_str();
            let var = inners.next().unwrap().as_str().to_string();
            let domain = parse_inv_term_from_expr(inners.next().unwrap())?;
            let body = parse_invariant_expr(inners.next().unwrap())?;
            match q_type {
                "forall" => Ok(InvariantExpr::Forall {
                    var, domain: Box::new(domain), body: Box::new(body),
                }),
                "exists" => Ok(InvariantExpr::Exists {
                    var, domain: Box::new(domain), body: Box::new(body),
                }),
                _ => Err("Unknown quantifier".into()),
            }
        },
        Rule::invariant_expr => {
            let inner = pair.into_inner().next().unwrap();
            parse_invariant_expr(inner)
        },
        _ => Ok(InvariantExpr::True),
    }
}

fn parse_inv_additive(pair: pest::iterators::Pair<Rule>) -> Result<InvTerm, String> {
    let mut inners: Vec<_> = pair.into_inner().collect();
    if inners.len() == 1 {
        return parse_inv_multiplicative(inners.remove(0));
    }
    let mut left = parse_inv_multiplicative(inners.remove(0))?;
    while inners.len() >= 2 {
        let op_str = inners.remove(0).as_str();
        let op = match op_str { "+" => ArithOp::Add, "-" => ArithOp::Sub, _ => ArithOp::Add };
        let right = parse_inv_multiplicative(inners.remove(0))?;
        left = InvTerm::BinOp { left: Box::new(left), op, right: Box::new(right) };
    }
    Ok(left)
}

fn parse_inv_multiplicative(pair: pest::iterators::Pair<Rule>) -> Result<InvTerm, String> {
    let mut inners: Vec<_> = pair.into_inner().collect();
    if inners.len() == 1 {
        return parse_inv_primary(inners.remove(0));
    }
    let mut left = parse_inv_primary(inners.remove(0))?;
    while inners.len() >= 2 {
        let op_str = inners.remove(0).as_str();
        let op = match op_str {
            "*" => ArithOp::Mul, "/" => ArithOp::Div, "%" => ArithOp::Mod,
            _ => ArithOp::Mul
        };
        let right = parse_inv_primary(inners.remove(0))?;
        left = InvTerm::BinOp { left: Box::new(left), op, right: Box::new(right) };
    }
    Ok(left)
}

fn parse_inv_primary(pair: pest::iterators::Pair<Rule>) -> Result<InvTerm, String> {
    match pair.as_rule() {
        Rule::num_lit => {
            let n: i128 = pair.as_str().parse().unwrap_or(0);
            Ok(InvTerm::Literal(n))
        },
        Rule::ident_post => {
            let s = pair.as_str();
            let is_post = s.ends_with('\'');
            let name = if is_post { s.trim_end_matches('\'').to_string() } else { s.to_string() };
            Ok(InvTerm::Var { name, is_post })
        },
        Rule::field_access_inv => {
            let s = pair.as_str();
            let is_post = s.ends_with('\'');
            let clean = s.trim_end_matches('\'');
            let parts: Vec<&str> = clean.split('.').collect();
            if parts.len() >= 2 {
                Ok(InvTerm::FieldAccess {
                    object: parts[0].to_string(),
                    field: parts[1].to_string(),
                    is_post,
                })
            } else {
                Ok(InvTerm::Var { name: clean.to_string(), is_post })
            }
        },
        Rule::paren_inv => {
            let inner = pair.into_inner().next().unwrap();
            parse_inv_additive(inner)
        },
        Rule::fn_call_inv => {
            let mut inners = pair.into_inner();
            let name = inners.next().unwrap().as_str().to_string();
            let args: Vec<InvTerm> = inners
                .filter(|p| p.as_rule() == Rule::additive)
                .map(|p| parse_inv_additive(p).unwrap_or(InvTerm::Literal(0)))
                .collect();
            Ok(InvTerm::FnCall { name, args })
        },
        Rule::neg_inv => {
            let inner = pair.into_inner().next().unwrap();
            let term = parse_inv_primary(inner)?;
            Ok(InvTerm::BinOp {
                left: Box::new(InvTerm::Literal(0)), op: ArithOp::Sub, right: Box::new(term),
            })
        },
        _ => {
            // Try as literal
            let s = pair.as_str().trim();
            if let Ok(n) = s.parse::<i128>() {
                Ok(InvTerm::Literal(n))
            } else {
                let is_post = s.ends_with('\'');
                let name = s.trim_end_matches('\'').to_string();
                Ok(InvTerm::Var { name, is_post })
            }
        }
    }
}

fn parse_inv_term_from_expr(pair: pest::iterators::Pair<Rule>) -> Result<InvTerm, String> {
    let s = pair.as_str().trim();
    if let Ok(n) = s.parse::<i128>() {
        Ok(InvTerm::Literal(n))
    } else {
        Ok(InvTerm::Var { name: s.to_string(), is_post: false })
    }
}

fn parse_cmp_op(pair: pest::iterators::Pair<Rule>) -> Result<CmpOp, String> {
    Ok(match pair.as_str() {
        "==" => CmpOp::Eq, "!=" => CmpOp::Neq,
        "<" => CmpOp::Lt, ">" => CmpOp::Gt,
        "<=" => CmpOp::Lte, ">=" => CmpOp::Gte,
        _ => CmpOp::Eq,
    })
}

fn parse_block(pair: pest::iterators::Pair<Rule>) -> Result<Block, String> {
    let mut stmts = Vec::new();
    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::let_stmt => stmts.push(parse_let_stmt(inner)?),
            Rule::assign_stmt => stmts.push(parse_assign_stmt(inner)?),
            Rule::return_stmt => stmts.push(parse_return_stmt(inner)?),
            Rule::assert_stmt => stmts.push(parse_assert_stmt(inner)?),
            Rule::expr_stmt => {
                let expr_inner = inner.into_inner().next().unwrap();
                stmts.push(Stmt::Expr(parse_expr(expr_inner)?));
            },
            _ => {},
        }
    }
    Ok(Block { stmts, expr: None })
}

fn parse_let_stmt(pair: pest::iterators::Pair<Rule>) -> Result<Stmt, String> {
    let mut name = String::new();
    let mut ty = None;
    let mut is_mut = false;
    let mut value = Expr::IntLit(0);

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::ident => {
                let s = inner.as_str();
                if s == "mut" { is_mut = true; } else { name = s.to_string(); }
            },
            Rule::type_expr => ty = Some(parse_type(inner)?),
            Rule::expr => value = parse_expr(inner)?,
            _ => {},
        }
    }
    Ok(Stmt::Let { name, ty, is_mut, value })
}

fn parse_assign_stmt(pair: pest::iterators::Pair<Rule>) -> Result<Stmt, String> {
    let mut inners: Vec<_> = pair.into_inner().collect();
    let lv = parse_lvalue(inners.remove(0))?;
    let op = match inners.remove(0).as_str() {
        "=" => AssignOp::Assign, "+=" => AssignOp::AddAssign,
        "-=" => AssignOp::SubAssign, "*=" => AssignOp::MulAssign,
        "/=" => AssignOp::DivAssign, _ => AssignOp::Assign,
    };
    let value = parse_expr(inners.remove(0))?;
    Ok(Stmt::Assign { target: lv, op, value })
}

fn parse_lvalue(pair: pest::iterators::Pair<Rule>) -> Result<LValue, String> {
    let s = pair.as_str().trim();
    if s.contains('.') {
        let parts: Vec<&str> = s.split('.').collect();
        Ok(LValue::FieldAccess { object: parts[0].trim().to_string(), field: parts[1].trim().to_string() })
    } else {
        Ok(LValue::Ident(s.to_string()))
    }
}

fn parse_return_stmt(pair: pest::iterators::Pair<Rule>) -> Result<Stmt, String> {
    let expr = pair.into_inner().next().map(|p| parse_expr(p).unwrap_or(Expr::IntLit(0)));
    Ok(Stmt::Return(expr))
}

fn parse_assert_stmt(pair: pest::iterators::Pair<Rule>) -> Result<Stmt, String> {
    let mut inners = pair.into_inner();
    let condition = parse_expr(inners.next().unwrap())?;
    let message = inners.next().map(|p| p.as_str().trim_matches('"').to_string());
    Ok(Stmt::Assert { condition, message })
}

fn parse_expr(pair: pest::iterators::Pair<Rule>) -> Result<Expr, String> {
    let mut inners: Vec<_> = pair.into_inner().collect();
    if inners.len() == 1 {
        return parse_unary(inners.remove(0));
    }
    let mut left = parse_unary(inners.remove(0))?;
    while inners.len() >= 2 {
        let op = parse_bin_op(inners.remove(0))?;
        let right = parse_unary(inners.remove(0))?;
        left = Expr::BinOp { left: Box::new(left), op, right: Box::new(right) };
    }
    Ok(left)
}

fn parse_unary(pair: pest::iterators::Pair<Rule>) -> Result<Expr, String> {
    match pair.as_rule() {
        Rule::neg => {
            let inner = pair.into_inner().next().unwrap();
            Ok(Expr::UnaryOp { op: UnaryOp::Neg, operand: Box::new(parse_primary(inner)?) })
        },
        Rule::not => {
            let inner = pair.into_inner().next().unwrap();
            Ok(Expr::UnaryOp { op: UnaryOp::Not, operand: Box::new(parse_primary(inner)?) })
        },
        _ => parse_primary(pair),
    }
}

fn parse_primary(pair: pest::iterators::Pair<Rule>) -> Result<Expr, String> {
    match pair.as_rule() {
        Rule::num_lit => {
            let s = pair.as_str();
            if s.contains('.') {
                Ok(Expr::FloatLit(s.parse().unwrap_or(0.0)))
            } else {
                Ok(Expr::IntLit(s.parse().unwrap_or(0)))
            }
        },
        Rule::bool_lit => Ok(Expr::BoolLit(pair.as_str() == "true")),
        Rule::string_lit => Ok(Expr::StringLit(pair.as_str().trim_matches('"').to_string())),
        Rule::hex_lit => Ok(Expr::HexLit(pair.as_str().to_string())),
        Rule::paren_expr => {
            let inner = pair.into_inner().next().unwrap();
            parse_expr(inner)
        },
        Rule::fn_call => {
            let mut inners = pair.into_inner();
            let name = inners.next().unwrap().as_str().to_string();
            let args: Vec<Expr> = inners
                .filter(|p| p.as_rule() == Rule::expr)
                .map(|p| parse_expr(p).unwrap_or(Expr::IntLit(0)))
                .collect();
            Ok(Expr::FnCall { name, args })
        },
        Rule::ident_expr | Rule::ident | Rule::ident_post => {
            Ok(Expr::Ident(pair.as_str().to_string()))
        },
        Rule::field_access => {
            let s = pair.as_str();
            let parts: Vec<&str> = s.split('.').collect();
            let mut expr = Expr::Ident(parts[0].to_string());
            for &field in &parts[1..] {
                expr = Expr::FieldAccess { object: Box::new(expr), field: field.to_string() };
            }
            Ok(expr)
        },
        _ => Ok(Expr::Ident(pair.as_str().to_string())),
    }
}

fn parse_bin_op(pair: pest::iterators::Pair<Rule>) -> Result<BinOp, String> {
    Ok(match pair.as_str() {
        "+" => BinOp::Add, "-" => BinOp::Sub, "*" => BinOp::Mul,
        "/" => BinOp::Div, "%" => BinOp::Mod, "==" => BinOp::Eq,
        "!=" => BinOp::Neq, "<" => BinOp::Lt, ">" => BinOp::Gt,
        "<=" => BinOp::Lte, ">=" => BinOp::Gte,
        "&&" => BinOp::And, "||" => BinOp::Or,
        _ => BinOp::Add,
    })
}

fn parse_struct_def(pair: pest::iterators::Pair<Rule>) -> Result<StructDef, String> {
    let mut name = String::new();
    let mut is_pub = false;
    let mut fields = Vec::new();
    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::visibility => is_pub = true,
            Rule::ident => name = inner.as_str().to_string(),
            Rule::field_list => {
                for f in inner.into_inner() {
                    if f.as_rule() == Rule::field {
                        let mut fname = String::new();
                        let mut fty = Type::Unit;
                        let mut fpub = false;
                        for fi in f.into_inner() {
                            match fi.as_rule() {
                                Rule::visibility => fpub = true,
                                Rule::ident => fname = fi.as_str().to_string(),
                                Rule::type_expr => fty = parse_type(fi)?,
                                _ => {},
                            }
                        }
                        fields.push(Field { name: fname, ty: fty, is_pub: fpub });
                    }
                }
            },
            _ => {},
        }
    }
    Ok(StructDef { name, is_pub, fields, span: None })
}

fn parse_const_def(pair: pest::iterators::Pair<Rule>) -> Result<ConstDef, String> {
    let mut name = String::new();
    let mut ty = Type::Unit;
    let mut value = Expr::IntLit(0);
    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::ident => name = inner.as_str().to_string(),
            Rule::type_expr => ty = parse_type(inner)?,
            Rule::expr => value = parse_expr(inner)?,
            _ => {},
        }
    }
    Ok(ConstDef { name, ty, value, span: None })
}

fn parse_contract_def(pair: pest::iterators::Pair<Rule>) -> Result<ContractDef, String> {
    let mut name = String::new();
    let mut state_vars = Vec::new();
    let mut functions = Vec::new();
    let mut invariants = Vec::new();

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::ident => name = inner.as_str().to_string(),
            Rule::contract_body => {
                for body_item in inner.into_inner() {
                    match body_item.as_rule() {
                        Rule::state_var => {
                            let mut sname = String::new();
                            let mut sty = Type::Unit;
                            let mut sdefault = None;
                            for si in body_item.into_inner() {
                                match si.as_rule() {
                                    Rule::ident => sname = si.as_str().to_string(),
                                    Rule::type_expr => sty = parse_type(si)?,
                                    Rule::expr => sdefault = Some(parse_expr(si)?),
                                    _ => {},
                                }
                            }
                            state_vars.push(StateVar { name: sname, ty: sty, default: sdefault, span: None });
                        },
                        Rule::fn_def => functions.push(parse_fn_def(body_item)?),
                        Rule::invariant_block => {
                            for inv in body_item.into_inner() {
                                if inv.as_rule() == Rule::invariant {
                                    invariants.push(parse_invariant(inv)?);
                                }
                            }
                        },
                        _ => {},
                    }
                }
            },
            _ => {},
        }
    }
    Ok(ContractDef { name, state_vars, functions, invariants, span: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_fn() {
        let src = r#"
fn add(a: u64, b: u64) -> u64
    where {
        a > 0,
        b > 0
    }
{
    return a;
}
"#;
        let program = parse_program(src);
        assert!(program.is_ok(), "Failed to parse: {:?}", program.err());
        let p = program.unwrap();
        assert_eq!(p.items.len(), 1);
        if let Item::Function(f) = &p.items[0] {
            assert_eq!(f.name, "add");
            assert_eq!(f.params.len(), 2);
            assert_eq!(f.invariants.len(), 2);
        } else {
            panic!("Expected function");
        }
    }
}
