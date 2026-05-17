// ============================================================
// ANVIL TYPE CHECKER — Bidirectional Type Inference
// "Integers have edges. Overflow is not a runtime surprise,
//  it's a compile-time impossibility."
//
// The type checker bridges the gap between mathematical
// abstraction (Z3 Int) and silicon reality (u64 has 2^64 states).
// Every unchecked cast is a thermodynamic lie.
// ============================================================

use crate::core::ast::*;
use colored::Colorize;
use std::collections::HashMap;
use tracing::debug;

/// Type environment: maps variable names to their declared types
#[derive(Debug, Clone)]
pub struct TypeEnv {
    bindings: HashMap<String, Type>,
    /// Constraints discovered during checking (e.g., "x must be >= 0 for u64")
    pub constraints: Vec<TypeConstraint>,
    /// Errors accumulated during checking
    pub errors: Vec<TypeError>,
    /// Warnings (non-fatal, informational)
    pub warnings: Vec<TypeWarning>,
}

#[derive(Debug, Clone)]
pub struct TypeConstraint {
    pub var_name: String,
    pub kind: ConstraintKind,
}

#[derive(Debug, Clone)]
pub enum ConstraintKind {
    /// Variable must be >= 0 (unsigned type)
    NonNegative,
    /// Variable must be < 2^bits (bounded unsigned)
    UpperBound { bits: u32 },
    /// Variable must be >= -(2^(bits-1)) and < 2^(bits-1) (signed)
    SignedBound { bits: u32 },
}

#[derive(Debug, Clone)]
pub struct TypeError {
    pub message: String,
    pub location: String,
}

#[derive(Debug, Clone)]
pub struct TypeWarning {
    pub message: String,
    pub location: String,
}

impl TypeEnv {
    pub fn new() -> Self {
        Self {
            bindings: HashMap::new(),
            constraints: Vec::new(),
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn bind(&mut self, name: String, ty: Type) {
        self.bindings.insert(name, ty);
    }

    pub fn lookup(&self, name: &str) -> Option<&Type> {
        self.bindings.get(name)
    }

    /// Register type constraints for Z3 based on the declared type
    fn register_constraints(&mut self, name: &str, ty: &Type) {
        match ty {
            Type::U8 => {
                self.constraints.push(TypeConstraint {
                    var_name: name.to_string(),
                    kind: ConstraintKind::NonNegative,
                });
                self.constraints.push(TypeConstraint {
                    var_name: name.to_string(),
                    kind: ConstraintKind::UpperBound { bits: 8 },
                });
            }
            Type::U16 => {
                self.constraints.push(TypeConstraint {
                    var_name: name.to_string(),
                    kind: ConstraintKind::NonNegative,
                });
                self.constraints.push(TypeConstraint {
                    var_name: name.to_string(),
                    kind: ConstraintKind::UpperBound { bits: 16 },
                });
            }
            Type::U32 => {
                self.constraints.push(TypeConstraint {
                    var_name: name.to_string(),
                    kind: ConstraintKind::NonNegative,
                });
                self.constraints.push(TypeConstraint {
                    var_name: name.to_string(),
                    kind: ConstraintKind::UpperBound { bits: 32 },
                });
            }
            Type::U64 => {
                self.constraints.push(TypeConstraint {
                    var_name: name.to_string(),
                    kind: ConstraintKind::NonNegative,
                });
                self.constraints.push(TypeConstraint {
                    var_name: name.to_string(),
                    kind: ConstraintKind::UpperBound { bits: 64 },
                });
            }
            Type::U128 => {
                self.constraints.push(TypeConstraint {
                    var_name: name.to_string(),
                    kind: ConstraintKind::NonNegative,
                });
                self.constraints.push(TypeConstraint {
                    var_name: name.to_string(),
                    kind: ConstraintKind::UpperBound { bits: 128 },
                });
            }
            Type::U256 => {
                self.constraints.push(TypeConstraint {
                    var_name: name.to_string(),
                    kind: ConstraintKind::NonNegative,
                });
                self.constraints.push(TypeConstraint {
                    var_name: name.to_string(),
                    kind: ConstraintKind::UpperBound { bits: 256 },
                });
            }
            Type::I8 => {
                self.constraints.push(TypeConstraint {
                    var_name: name.to_string(),
                    kind: ConstraintKind::SignedBound { bits: 8 },
                });
            }
            Type::I16 => {
                self.constraints.push(TypeConstraint {
                    var_name: name.to_string(),
                    kind: ConstraintKind::SignedBound { bits: 16 },
                });
            }
            Type::I32 => {
                self.constraints.push(TypeConstraint {
                    var_name: name.to_string(),
                    kind: ConstraintKind::SignedBound { bits: 32 },
                });
            }
            Type::I64 => {
                self.constraints.push(TypeConstraint {
                    var_name: name.to_string(),
                    kind: ConstraintKind::SignedBound { bits: 64 },
                });
            }
            Type::I128 => {
                self.constraints.push(TypeConstraint {
                    var_name: name.to_string(),
                    kind: ConstraintKind::SignedBound { bits: 128 },
                });
            }
            // Bool, Address, String, etc. — no numeric constraints
            // Sovereign on-chain types — mapped to fixed-size byte layouts
            Type::Wallet | Type::TxHash => {
                // 32-byte = 256-bit identifier, treated as u256 for Z3
                self.constraints.push(TypeConstraint {
                    var_name: name.to_string(),
                    kind: ConstraintKind::NonNegative,
                });
                self.constraints.push(TypeConstraint {
                    var_name: name.to_string(),
                    kind: ConstraintKind::UpperBound { bits: 256 },
                });
            }
            Type::Signature => {
                // 65-byte ECDSA signature — no numeric constraints (opaque)
            }
            Type::Gas => {
                // Gas is u64-bounded
                self.constraints.push(TypeConstraint {
                    var_name: name.to_string(),
                    kind: ConstraintKind::NonNegative,
                });
                self.constraints.push(TypeConstraint {
                    var_name: name.to_string(),
                    kind: ConstraintKind::UpperBound { bits: 64 },
                });
            }
            _ => {}
        }
    }
}

/// Check an entire program for type consistency
pub fn check_program(program: &Program) -> TypeEnv {
    let mut env = TypeEnv::new();

    for item in &program.items {
        match item {
            Item::Function(f) => check_function(f, &mut env),
            Item::Contract(c) => check_contract(c, &mut env),
            Item::Struct(_) => {} // Struct definitions are type declarations
            Item::Const(c) => check_const(c, &mut env),
            Item::GhostVar(g) => {
                // Ghost variables: bind in proof domain
                env.bind(g.name.clone(), g.ty.clone());
                env.register_constraints(&g.name, &g.ty);
            },
        }
    }

    debug!(
        bindings = env.bindings.len(),
        constraints = env.constraints.len(),
        errors = env.errors.len(),
        warnings = env.warnings.len(),
        "Type checking complete"
    );

    env
}

fn check_function(func: &FnDef, env: &mut TypeEnv) {
    // Bind params and register constraints
    for param in &func.params {
        env.bind(param.name.clone(), param.ty.clone());
        env.register_constraints(&param.name, &param.ty);
    }

    // Check body statements
    check_block(&func.body, env, &func.name);

    // Verify return type matches body
    if let (Some(ret_ty), Some(Stmt::Return(Some(expr)))) = (&func.return_type, func.body.stmts.last()) {
        let expr_ty = infer_expr_type(expr, env);
        if let Some(ety) = &expr_ty {
            if !types_compatible(ety, ret_ty) {
                env.errors.push(TypeError {
                    message: format!(
                        "Return type mismatch in '{}': expected {}, got {}",
                        func.name,
                        format_type(ret_ty),
                        format_type(ety)
                    ),
                    location: func.name.clone(),
                });
            }
        }
    }

    // Check for potential overflow in assignments
    check_overflow_safety(func, env);
}

fn check_contract(contract: &ContractDef, env: &mut TypeEnv) {
    // Bind state variables
    for sv in &contract.state_vars {
        env.bind(sv.name.clone(), sv.ty.clone());
        env.register_constraints(&sv.name, &sv.ty);
    }

    // Check each function
    for func in &contract.functions {
        check_function(func, env);
    }
}

fn check_const(c: &ConstDef, env: &mut TypeEnv) {
    env.bind(c.name.clone(), c.ty.clone());
    let expr_ty = infer_expr_type(&c.value, env);
    if let Some(ety) = &expr_ty {
        if !types_compatible(ety, &c.ty) {
            env.errors.push(TypeError {
                message: format!(
                    "Const '{}' declared as {} but initialized with {}",
                    c.name,
                    format_type(&c.ty),
                    format_type(ety)
                ),
                location: c.name.clone(),
            });
        }
    }
}

fn check_block(block: &Block, env: &mut TypeEnv, fn_name: &str) {
    for stmt in &block.stmts {
        match stmt {
            Stmt::Let { name, ty, value, .. } => {
                let val_ty = infer_expr_type(value, env);
                if let Some(declared) = ty {
                    if let Some(ref inferred) = val_ty {
                        if !types_compatible(inferred, declared) {
                            env.errors.push(TypeError {
                                message: format!(
                                    "Type mismatch in 'let {}': declared {}, got {}",
                                    name,
                                    format_type(declared),
                                    format_type(inferred)
                                ),
                                location: fn_name.to_string(),
                            });
                        }
                    }
                    env.bind(name.clone(), declared.clone());
                    env.register_constraints(name, declared);
                } else if let Some(ref inferred) = val_ty {
                    env.bind(name.clone(), inferred.clone());
                    env.register_constraints(name, inferred);
                }
            }
            Stmt::Assign { target, value, .. } => {
                let target_name = match target {
                    LValue::Ident(n) => Some(n.as_str()),
                    _ => None,
                };
                if let Some(name) = target_name {
                    let target_ty = env.lookup(name).cloned();
                    let val_ty = infer_expr_type(value, env);
                    if let (Some(tt), Some(vt)) = (&target_ty, &val_ty) {
                        if !types_compatible(vt, tt) {
                            env.errors.push(TypeError {
                                message: format!(
                                    "Assignment type mismatch for '{}': expected {}, got {}",
                                    name,
                                    format_type(tt),
                                    format_type(vt)
                                ),
                                location: fn_name.to_string(),
                            });
                        }
                    }
                }
            }
            Stmt::Return(Some(expr)) => {
                let _ = infer_expr_type(expr, env);
            }
            Stmt::Ghost { name, ty, value, .. } => {
                // Ghost variables: bind in proof domain only
                let val_ty = infer_expr_type(value, env);
                if let Some(ref inferred) = val_ty {
                    if !types_compatible(inferred, ty) {
                        env.warnings.push(TypeWarning {
                            message: format!(
                                "Ghost variable '{}' type mismatch: declared {}, inferred {}",
                                name, format_type(ty), format_type(inferred)
                            ),
                            location: fn_name.to_string(),
                        });
                    }
                }
                env.bind(name.clone(), ty.clone());
                env.register_constraints(name, ty);
            }
            Stmt::Emit { .. } => {
                // Emit statements are passthrough for type checking
            }
            _ => {}
        }
    }
}

/// Infer the type of an expression from the environment
fn infer_expr_type(expr: &Expr, env: &TypeEnv) -> Option<Type> {
    match expr {
        Expr::IntLit(n) => {
            // Infer minimal type from literal value
            if *n >= 0 && *n <= 255 { Some(Type::U8) }
            else if *n >= 0 && *n <= 65535 { Some(Type::U16) }
            else if *n >= 0 && *n <= 4294967295 { Some(Type::U32) }
            else if *n >= 0 { Some(Type::U64) }
            else { Some(Type::I64) }
        }
        Expr::FloatLit(_) => None, // No float type yet
        Expr::BoolLit(_) => Some(Type::Bool),
        Expr::StringLit(_) => Some(Type::String),
        Expr::Ident(name) => env.lookup(name).cloned(),
        Expr::BinOp { left, op, right } => {
            let lt = infer_expr_type(left, env);
            let rt = infer_expr_type(right, env);
            match op {
                BinOp::Eq | BinOp::Neq | BinOp::Lt | BinOp::Gt |
                BinOp::Lte | BinOp::Gte | BinOp::And | BinOp::Or => Some(Type::Bool),
                _ => {
                    // Arithmetic: promote to wider type
                    match (lt, rt) {
                        (Some(l), Some(r)) => Some(promote_types(&l, &r)),
                        (Some(t), None) | (None, Some(t)) => Some(t),
                        _ => None,
                    }
                }
            }
        }
        Expr::UnaryOp { op, operand } => {
            match op {
                UnaryOp::Not => Some(Type::Bool),
                UnaryOp::Neg => infer_expr_type(operand, env),
            }
        }
        Expr::FnCall { name, .. } => {
            // Would need function signature registry for full inference
            // For now, return None (unknown)
            let _ = name;
            None
        }
        _ => None,
    }
}

/// Check for potential overflow in arithmetic operations on unsigned types
fn check_overflow_safety(func: &FnDef, env: &mut TypeEnv) {
    for stmt in &func.body.stmts {
        if let Stmt::Assign { target, op, value } = stmt {
            let target_name = match target {
                LValue::Ident(n) => n.clone(),
                LValue::FieldAccess { object, field } => format!("{}.{}", object, field),
                _ => continue,
            };

            let target_ty = env.lookup(&target_name).cloned();

            // Check SubAssign on unsigned types — potential underflow
            if matches!(op, AssignOp::SubAssign) {
                if let Some(ty) = target_ty.as_ref().filter(|t| is_unsigned(t)) {
                    // Check if the invariants already guard against this
                    let has_guard = func.invariants.iter().any(|inv| {
                        invariant_guards_underflow(&inv.expr, &target_name, value)
                    });

                    if !has_guard {
                        env.warnings.push(TypeWarning {
                            message: format!(
                                "Potential underflow: '{} -= ...' on {} without explicit guard. \
                                 Consider adding '{} >= <value>' to where clause.",
                                target_name,
                                format_type(ty),
                                target_name,
                            ),
                            location: func.name.clone(),
                        });
                    }
                }
            }

            // Check AddAssign on unsigned types — potential overflow
            if matches!(op, AssignOp::AddAssign) {
                if let Some(ty) = target_ty.as_ref().filter(|t| is_unsigned(t)) {
                    env.warnings.push(TypeWarning {
                        message: format!(
                            "Potential overflow: '{} += ...' on {}. \
                             Post-state will be bounded by Z3 type constraints.",
                            target_name,
                            format_type(ty),
                        ),
                        location: func.name.clone(),
                    });
                }
            }
        }
    }
}

/// Check if an invariant expression guards against underflow for a given variable
fn invariant_guards_underflow(inv: &InvariantExpr, var_name: &str, _value: &Expr) -> bool {
    match inv {
        InvariantExpr::Comparison { left, op, right } => {
            // Pattern: var >= value (guards underflow)
            match (left.as_ref(), op, right.as_ref()) {
                (InvTerm::Var { name, is_post: false }, CmpOp::Gte, _) if name == var_name => true,
                (_, CmpOp::Lte, InvTerm::Var { name, is_post: false }) if name == var_name => true,
                _ => false,
            }
        }
        InvariantExpr::And(a, b) => {
            invariant_guards_underflow(a, var_name, _value)
                || invariant_guards_underflow(b, var_name, _value)
        }
        _ => false,
    }
}

// --- Type utilities ---

fn is_unsigned(ty: &Type) -> bool {
    matches!(ty, Type::U8 | Type::U16 | Type::U32 | Type::U64 | Type::U128 | Type::U256 | Type::Gas)
}

fn types_compatible(got: &Type, expected: &Type) -> bool {
    if got == expected { return true; }
    // Allow implicit widening: u8 → u16 → u32 → u64 → u128
    let got_rank = type_rank(got);
    let expected_rank = type_rank(expected);
    if got_rank > 0 && expected_rank > 0 {
        // Both are numeric — allow widening
        return got_rank <= expected_rank && same_signedness(got, expected);
    }
    false
}

fn same_signedness(a: &Type, b: &Type) -> bool {
    (is_unsigned(a) && is_unsigned(b)) || (is_signed(a) && is_signed(b))
}

fn is_signed(ty: &Type) -> bool {
    matches!(ty, Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::I128)
}

fn type_rank(ty: &Type) -> u32 {
    match ty {
        Type::U8 | Type::I8 => 1,
        Type::U16 | Type::I16 => 2,
        Type::U32 | Type::I32 => 3,
        Type::U64 | Type::I64 => 4,
        Type::U128 | Type::I128 => 5,
        Type::U256 => 6,
        Type::Bool => 0,
        _ => 0,
    }
}

fn promote_types(a: &Type, b: &Type) -> Type {
    let ra = type_rank(a);
    let rb = type_rank(b);
    if ra >= rb { a.clone() } else { b.clone() }
}

fn format_type(ty: &Type) -> String {
    match ty {
        Type::U8 => "u8".into(), Type::U16 => "u16".into(),
        Type::U32 => "u32".into(), Type::U64 => "u64".into(),
        Type::U128 => "u128".into(), Type::U256 => "u256".into(),
        Type::I8 => "i8".into(), Type::I16 => "i16".into(),
        Type::I32 => "i32".into(), Type::I64 => "i64".into(),
        Type::I128 => "i128".into(),
        Type::Bool => "bool".into(), Type::Address => "Address".into(),
        Type::String => "String".into(), Type::Unit => "()".into(),
        Type::Wallet => "Wallet".into(), Type::Signature => "Signature".into(),
        Type::TxHash => "TxHash".into(), Type::Gas => "Gas".into(),
        Type::Array(t) => format!("[{}]", format_type(t)),
        Type::Map(k, v) => format!("Map<{}, {}>", format_type(k), format_type(v)),
        Type::Option(t) => format!("Option<{}>", format_type(t)),
        Type::Result(t, e) => format!("Result<{}, {}>", format_type(t), format_type(e)),
        Type::Named(n) => n.clone(),
    }
}

// --- Pretty printing ---

pub fn print_type_report(env: &TypeEnv) {
    if !env.errors.is_empty() {
        eprintln!();
        eprintln!("{}", "╔══════════════════════════════════════════════════╗".bright_red());
        eprintln!("{}", "║         ANVIL TYPE ERRORS                        ║".bright_red());
        eprintln!("{}", "╚══════════════════════════════════════════════════╝".bright_red());
        eprintln!();
        for err in &env.errors {
            eprintln!("  {} [{}] {}", "✗".bright_red().bold(), err.location, err.message);
        }
    }

    if !env.warnings.is_empty() {
        eprintln!();
        for warn in &env.warnings {
            eprintln!("  {} [{}] {}", "⚠".bright_yellow(), warn.location, warn.message);
        }
    }

    if !env.constraints.is_empty() && env.errors.is_empty() {
        eprintln!("  {} {} type constraints registered for Z3",
            "✓".bright_green(), env.constraints.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser;

    #[test]
    fn test_type_check_transfer() {
        let src = r#"
fn transfer(sender_balance: u64, receiver_balance: u64, amount: u64) -> u64
    where {
        amount > 0,
        sender_balance >= amount,
        sender_balance' + receiver_balance' == sender_balance + receiver_balance,
        sender_balance' == sender_balance - amount,
        receiver_balance' == receiver_balance + amount
    }
{
    sender_balance -= amount;
    receiver_balance += amount;
    return sender_balance;
}
"#;
        let program = parser::parse_program(src).unwrap();
        let env = check_program(&program);
        assert!(env.errors.is_empty(), "Unexpected type errors: {:?}", env.errors);
        // Should have constraints for sender_balance, receiver_balance, amount (2 each: non-neg + upper bound)
        assert!(env.constraints.len() >= 6, "Expected >= 6 constraints, got {}", env.constraints.len());
    }

    #[test]
    fn test_unsigned_constraints() {
        let src = r#"
fn simple(x: u64) -> u64
    where { x > 0 }
{
    return x;
}
"#;
        let program = parser::parse_program(src).unwrap();
        let env = check_program(&program);
        assert!(env.errors.is_empty());
        // u64 generates NonNegative + UpperBound(64)
        assert_eq!(env.constraints.len(), 2);
    }
}
