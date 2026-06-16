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
    scopes: Vec<HashMap<String, Type>>,
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
            scopes: Vec::new(),
            constraints: Vec::new(),
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn bind(&mut self, name: String, ty: Type) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, ty);
        } else {
            self.bindings.insert(name, ty);
        }
    }

    pub fn lookup(&self, name: &str) -> Option<&Type> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty);
            }
        }
        self.bindings.get(name)
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
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

impl Default for TypeEnv {
    fn default() -> Self {
        Self::new()
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
                check_declared_expr(
                    &g.name,
                    &g.ty,
                    &g.value,
                    &g.name,
                    "Ghost variable",
                    &mut env,
                );
                env.bind(g.name.clone(), g.ty.clone());
                env.register_constraints(&g.name, &g.ty);
            }
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
    env.push_scope();

    // Bind params and register constraints
    for param in &func.params {
        env.bind(param.name.clone(), param.ty.clone());
        env.register_constraints(&param.name, &param.ty);
    }

    // Check body statements
    check_block(&func.body, env, &func.name, func.return_type.as_ref());

    // Check all nested expressions for valid allocations and type safety
    check_block_exprs(&func.body, env, &func.name);

    // Check for potential overflow in assignments
    check_overflow_safety(func, env);

    env.pop_scope();
}

fn check_contract(contract: &ContractDef, env: &mut TypeEnv) {
    env.push_scope();

    // Bind state variables
    for sv in &contract.state_vars {
        if let Some(default) = &sv.default {
            check_declared_expr(
                &sv.name,
                &sv.ty,
                default,
                &contract.name,
                "State variable",
                env,
            );
        }
        env.bind(sv.name.clone(), sv.ty.clone());
        env.register_constraints(&sv.name, &sv.ty);
    }

    // Check each function
    for func in &contract.functions {
        check_function(func, env);
    }

    env.pop_scope();
}

fn check_const(c: &ConstDef, env: &mut TypeEnv) {
    check_declared_expr(&c.name, &c.ty, &c.value, &c.name, "Const", env);
    env.bind(c.name.clone(), c.ty.clone());
}

fn check_block(block: &Block, env: &mut TypeEnv, fn_name: &str, expected_return: Option<&Type>) {
    env.push_scope();

    for stmt in &block.stmts {
        match stmt {
            Stmt::Let {
                name, ty, value, ..
            } => {
                if let Some(undefined) = undefined_ident(value, env) {
                    env.errors.push(TypeError {
                        message: format!("Use of undefined variable '{}'", undefined),
                        location: fn_name.to_string(),
                    });
                }
                if let Some(declared) = ty {
                    check_declared_expr(name, declared, value, fn_name, "let", env);
                    env.bind(name.clone(), declared.clone());
                    env.register_constraints(name, declared);
                } else if let Some(ref inferred) = infer_expr_type(value, env) {
                    env.bind(name.clone(), inferred.clone());
                    env.register_constraints(name, inferred);
                }
            }
            Stmt::Assign { target, value, .. } => {
                if let Some(undefined) = undefined_ident(value, env) {
                    env.errors.push(TypeError {
                        message: format!("Use of undefined variable '{}'", undefined),
                        location: fn_name.to_string(),
                    });
                }
                let target_name = match target {
                    LValue::Ident(n) => Some(n.as_str()),
                    _ => None,
                };
                if let Some(name) = target_name {
                    let target_ty = env.lookup(name).cloned();
                    match &target_ty {
                        None => env.errors.push(TypeError {
                            message: format!("Assignment to undefined variable '{}'", name),
                            location: fn_name.to_string(),
                        }),
                        Some(tt) => {
                            check_declared_expr(name, tt, value, fn_name, "Assignment", env);
                        }
                    }
                }
            }
            Stmt::Return(Some(expr)) => {
                if let Some(undefined) = undefined_ident(expr, env) {
                    env.errors.push(TypeError {
                        message: format!("Use of undefined variable '{}'", undefined),
                        location: fn_name.to_string(),
                    });
                }
                if let Some(expected) = expected_return {
                    check_declared_expr(fn_name, expected, expr, fn_name, "Return", env);
                }
            }
            Stmt::Ghost {
                name, ty, value, ..
            } => {
                // Ghost variables: bind in proof domain only
                if let Some(undefined) = undefined_ident(value, env) {
                    env.errors.push(TypeError {
                        message: format!("Use of undefined variable '{}'", undefined),
                        location: fn_name.to_string(),
                    });
                }
                check_declared_expr(name, ty, value, fn_name, "Ghost variable", env);
                env.bind(name.clone(), ty.clone());
                env.register_constraints(name, ty);
            }
            Stmt::Emit { .. } => {
                // Emit statements are passthrough for type checking
            }
            Stmt::Assert { condition, .. } => {
                check_bool_expr(condition, env, fn_name, "assert condition");
            }
            Stmt::If {
                condition,
                then_block,
                else_block,
            } => {
                check_bool_expr(condition, env, fn_name, "if condition");
                check_block(then_block, env, fn_name, expected_return);
                if let Some(else_block) = else_block {
                    check_block(else_block, env, fn_name, expected_return);
                }
            }
            Stmt::While {
                condition, body, ..
            } => {
                check_bool_expr(condition, env, fn_name, "while condition");
                check_block(body, env, fn_name, expected_return);
            }
            _ => {}
        }
    }

    env.pop_scope();
}

fn check_bool_expr(expr: &Expr, env: &mut TypeEnv, fn_name: &str, label: &str) {
    if let Some(undefined) = undefined_ident(expr, env) {
        env.errors.push(TypeError {
            message: format!("Use of undefined variable '{}'", undefined),
            location: fn_name.to_string(),
        });
        return;
    }
    if let Some(ty) = infer_expr_type(expr, env) {
        if !types_compatible(&ty, &Type::Bool) {
            env.errors.push(TypeError {
                message: format!("{} must be bool, got {}", label, format_type(&ty)),
                location: fn_name.to_string(),
            });
        }
    }
}

fn check_declared_expr(
    name: &str,
    expected: &Type,
    expr: &Expr,
    location: &str,
    label: &str,
    env: &mut TypeEnv,
) {
    if expr_fits_type(expr, expected) {
        return;
    }

    if let Some(actual) = infer_expr_type(expr, env) {
        if types_compatible(&actual, expected) {
            return;
        }
        env.errors.push(TypeError {
            message: format!(
                "{} '{}' declared as {} but initialized with {}",
                label,
                name,
                format_type(expected),
                format_type(&actual)
            ),
            location: location.to_string(),
        });
    } else {
        if expr_contains_fncall(expr) {
            return;
        }
        env.errors.push(TypeError {
            message: format!(
                "{} '{}' declared as {} but initialized with incompatible expression",
                label,
                name,
                format_type(expected)
            ),
            location: location.to_string(),
        });
    }
}

fn expr_contains_fncall(expr: &Expr) -> bool {
    match expr {
        Expr::FnCall { .. } => true,
        Expr::BinOp { left, right, .. } => {
            expr_contains_fncall(left) || expr_contains_fncall(right)
        }
        Expr::UnaryOp { operand, .. } => expr_contains_fncall(operand),
        Expr::MethodCall { object, args, .. } => {
            expr_contains_fncall(object) || args.iter().any(expr_contains_fncall)
        }
        Expr::FieldAccess { object, .. } => expr_contains_fncall(object),
        Expr::Index { object, index } => {
            expr_contains_fncall(object) || expr_contains_fncall(index)
        }
        Expr::If {
            condition,
            then_block,
            else_block,
        } => {
            expr_contains_fncall(condition)
                || block_contains_fncall(then_block)
                || else_block.as_ref().is_some_and(block_contains_fncall)
        }
        Expr::Block(block) => block_contains_fncall(block),
        Expr::Alloc { arena, value } => {
            expr_contains_fncall(arena) || expr_contains_fncall(value)
        }
        _ => false,
    }
}

fn block_contains_fncall(block: &Block) -> bool {
    block.stmts.iter().any(|stmt| match stmt {
        Stmt::Let { value, .. } | Stmt::Assign { value, .. } | Stmt::Ghost { value, .. } => {
            expr_contains_fncall(value)
        }
        Stmt::Return(Some(expr))
        | Stmt::Assert {
            condition: expr, ..
        } => expr_contains_fncall(expr),
        Stmt::If {
            condition,
            then_block,
            else_block,
        } => {
            expr_contains_fncall(condition)
                || block_contains_fncall(then_block)
                || else_block.as_ref().is_some_and(block_contains_fncall)
        }
        Stmt::While {
            condition, body, ..
        } => expr_contains_fncall(condition) || block_contains_fncall(body),
        Stmt::Emit { args, .. } => args.iter().any(expr_contains_fncall),
        Stmt::Expr(expr) => expr_contains_fncall(expr),
        _ => false,
    })
}

fn undefined_ident(expr: &Expr, env: &TypeEnv) -> Option<String> {
    match expr {
        Expr::Ident(name) => env.lookup(name).is_none().then(|| name.clone()),
        Expr::BinOp { left, right, .. } => {
            undefined_ident(left, env).or_else(|| undefined_ident(right, env))
        }
        Expr::UnaryOp { operand, .. } => undefined_ident(operand, env),
        Expr::FnCall { args, .. } => args.iter().find_map(|arg| undefined_ident(arg, env)),
        Expr::MethodCall { object, args, .. } => undefined_ident(object, env)
            .or_else(|| args.iter().find_map(|arg| undefined_ident(arg, env))),
        Expr::FieldAccess { object, .. } => undefined_ident(object, env),
        Expr::Index { object, index } => {
            undefined_ident(object, env).or_else(|| undefined_ident(index, env))
        }
        Expr::If {
            condition,
            then_block,
            else_block,
        } => undefined_ident(condition, env)
            .or_else(|| undefined_ident_in_block(then_block, env))
            .or_else(|| {
                else_block
                    .as_ref()
                    .and_then(|block| undefined_ident_in_block(block, env))
            }),
        Expr::Block(block) => undefined_ident_in_block(block, env),
        Expr::Alloc { arena, value } => {
            undefined_ident(arena, env).or_else(|| undefined_ident(value, env))
        }
        _ => None,
    }
}

fn undefined_ident_in_block(block: &Block, env: &TypeEnv) -> Option<String> {
    block.stmts.iter().find_map(|stmt| match stmt {
        Stmt::Let { value, .. } | Stmt::Assign { value, .. } | Stmt::Ghost { value, .. } => {
            undefined_ident(value, env)
        }
        Stmt::Return(Some(expr))
        | Stmt::Assert {
            condition: expr, ..
        } => undefined_ident(expr, env),
        Stmt::If {
            condition,
            then_block,
            else_block,
        } => undefined_ident(condition, env)
            .or_else(|| undefined_ident_in_block(then_block, env))
            .or_else(|| {
                else_block
                    .as_ref()
                    .and_then(|block| undefined_ident_in_block(block, env))
            }),
        Stmt::While {
            condition, body, ..
        } => undefined_ident(condition, env).or_else(|| undefined_ident_in_block(body, env)),
        _ => None,
    })
}

const U256_MAX_DEC: &str =
    "115792089237316195423570985008687907853269984665640564039457584007913129639935";
const I128_MIN_ABS_DEC: &str = "170141183460469231731687303715884105728";

fn decimal_pow2(exp: u32) -> String {
    let mut digits = vec![1u8];
    for _ in 0..exp {
        let mut carry = 0u8;
        for digit in &mut digits {
            let doubled = *digit * 2 + carry;
            *digit = doubled % 10;
            carry = doubled / 10;
        }
        if carry > 0 {
            digits.push(carry);
        }
    }
    digits.iter().rev().map(|d| char::from(b'0' + *d)).collect()
}

fn decimal_minus_one(value: &str) -> String {
    let mut digits: Vec<u8> = value.bytes().rev().map(|b| b - b'0').collect();
    for digit in &mut digits {
        if *digit > 0 {
            *digit -= 1;
            break;
        }
        *digit = 9;
    }
    while digits.len() > 1 && digits.last() == Some(&0) {
        digits.pop();
    }
    digits.iter().rev().map(|d| char::from(b'0' + *d)).collect()
}

fn canonical_decimal(s: &str) -> String {
    let trimmed = s.trim_start_matches('0');
    if trimmed.is_empty() {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

fn decimal_lte(a: &str, b: &str) -> bool {
    let a = canonical_decimal(a);
    let b = canonical_decimal(b);
    a.len() < b.len() || (a.len() == b.len() && a <= b)
}

fn literal_decimal_value(expr: &Expr) -> Option<(bool, String)> {
    match expr {
        Expr::IntLit(n) if *n < 0 => Some((true, n.unsigned_abs().to_string())),
        Expr::IntLit(n) => Some((false, n.to_string())),
        Expr::BigIntLit(n) => Some((false, canonical_decimal(n))),
        Expr::UnaryOp {
            op: UnaryOp::Neg,
            operand,
        } => {
            let (negative, magnitude) = literal_decimal_value(operand)?;
            if magnitude == "0" {
                Some((false, magnitude))
            } else {
                Some((!negative, magnitude))
            }
        }
        _ => None,
    }
}

fn unsigned_bits(ty: &Type) -> Option<u32> {
    match ty {
        Type::U8 => Some(8),
        Type::U16 => Some(16),
        Type::U32 => Some(32),
        Type::U64 | Type::Gas => Some(64),
        Type::U128 => Some(128),
        Type::U256 | Type::Wallet | Type::TxHash => Some(256),
        _ => None,
    }
}

fn signed_bits(ty: &Type) -> Option<u32> {
    match ty {
        Type::I8 => Some(8),
        Type::I16 => Some(16),
        Type::I32 => Some(32),
        Type::I64 => Some(64),
        Type::I128 => Some(128),
        _ => None,
    }
}

fn expr_fits_type(expr: &Expr, expected: &Type) -> bool {
    match (expr, expected) {
        (Expr::BoolLit(_), Type::Bool)
        | (Expr::StringLit(_), Type::String)
        | (Expr::AddressLit(_), Type::Address) => return true,
        _ => {}
    }

    let Some((negative, magnitude)) = literal_decimal_value(expr) else {
        return false;
    };

    if let Some(bits) = unsigned_bits(expected) {
        if negative {
            return false;
        }
        let max = if bits == 256 {
            U256_MAX_DEC.to_string()
        } else {
            decimal_minus_one(&decimal_pow2(bits))
        };
        return decimal_lte(&magnitude, &max);
    }

    if let Some(bits) = signed_bits(expected) {
        let min_abs = if bits == 128 {
            I128_MIN_ABS_DEC.to_string()
        } else {
            decimal_pow2(bits - 1)
        };
        if negative {
            return decimal_lte(&magnitude, &min_abs);
        }
        return decimal_lte(&magnitude, &decimal_minus_one(&min_abs));
    }

    false
}

/// Infer the type of an expression from the environment
fn infer_expr_type(expr: &Expr, env: &TypeEnv) -> Option<Type> {
    match expr {
        Expr::IntLit(n) => {
            // Infer minimal type from literal value
            if *n >= 0 && *n <= 255 {
                Some(Type::U8)
            } else if *n >= 0 && *n <= 65535 {
                Some(Type::U16)
            } else if *n >= 0 && *n <= 4294967295 {
                Some(Type::U32)
            } else if *n >= 0 && *n <= u64::MAX as i128 {
                Some(Type::U64)
            } else if *n >= 0 {
                Some(Type::U128)
            } else if *n >= i64::MIN as i128 {
                Some(Type::I64)
            } else {
                Some(Type::I128)
            }
        }
        Expr::BigIntLit(_) => Some(Type::U256),
        Expr::FloatLit(_) => None, // No float type yet
        Expr::BoolLit(_) => Some(Type::Bool),
        Expr::StringLit(_) => Some(Type::String),
        Expr::AddressLit(_) => Some(Type::Address),
        Expr::HexLit(_) => Some(Type::U256),
        Expr::Ident(name) => env.lookup(name).cloned(),
        Expr::BinOp { left, op, right } => {
            let lt = infer_expr_type(left, env);
            let rt = infer_expr_type(right, env);
            match op {
                BinOp::Eq
                | BinOp::Neq
                | BinOp::Lt
                | BinOp::Gt
                | BinOp::Lte
                | BinOp::Gte
                | BinOp::And
                | BinOp::Or => Some(Type::Bool),
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
        Expr::UnaryOp { op, operand } => match op {
            UnaryOp::Not => Some(Type::Bool),
            UnaryOp::Neg => infer_negated_expr_type(operand, env),
        },
        Expr::FnCall { name, .. } => {
            // Would need function signature registry for full inference
            // For now, return None (unknown)
            let _ = name;
            None
        }
        Expr::Alloc { arena, value } => {
            let arena_ty = infer_expr_type(arena, env);
            match arena_ty {
                Some(Type::Arena(_)) => {
                    if let Some(value_ty) = infer_expr_type(value, env) {
                        Some(Type::Named(format!("*{}", format_type(&value_ty))))
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn infer_negated_expr_type(expr: &Expr, env: &TypeEnv) -> Option<Type> {
    match expr {
        Expr::IntLit(n) if *n >= 0 && *n <= 128 => Some(Type::I8),
        Expr::IntLit(n) if *n >= 0 && *n <= 32768 => Some(Type::I16),
        Expr::IntLit(n) if *n >= 0 && *n <= 2147483648 => Some(Type::I32),
        Expr::IntLit(n) if *n >= 0 && *n <= 9223372036854775808_i128 => Some(Type::I64),
        Expr::IntLit(_) => Some(Type::I128),
        Expr::BigIntLit(n) if decimal_lte(n, I128_MIN_ABS_DEC) => Some(Type::I128),
        Expr::BigIntLit(_) => None,
        _ => infer_expr_type(expr, env).filter(is_signed),
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
                    let has_guard = func
                        .invariants
                        .iter()
                        .any(|inv| invariant_guards_underflow(&inv.expr, &target_name, value));

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
                (
                    InvTerm::Var {
                        name,
                        is_post: false,
                    },
                    CmpOp::Gte,
                    _,
                ) if name == var_name => true,
                (
                    _,
                    CmpOp::Lte,
                    InvTerm::Var {
                        name,
                        is_post: false,
                    },
                ) if name == var_name => true,
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
    matches!(
        ty,
        Type::U8 | Type::U16 | Type::U32 | Type::U64 | Type::U128 | Type::U256 | Type::Gas
    )
}

fn types_compatible(got: &Type, expected: &Type) -> bool {
    if got == expected {
        return true;
    }
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
    matches!(
        ty,
        Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::I128
    )
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
        Type::U8 => "u8".into(),
        Type::U16 => "u16".into(),
        Type::U32 => "u32".into(),
        Type::U64 => "u64".into(),
        Type::U128 => "u128".into(),
        Type::U256 => "u256".into(),
        Type::I8 => "i8".into(),
        Type::I16 => "i16".into(),
        Type::I32 => "i32".into(),
        Type::I64 => "i64".into(),
        Type::I128 => "i128".into(),
        Type::Bool => "bool".into(),
        Type::Address => "Address".into(),
        Type::String => "String".into(),
        Type::Unit => "()".into(),
        Type::Wallet => "Wallet".into(),
        Type::Signature => "Signature".into(),
        Type::TxHash => "TxHash".into(),
        Type::Gas => "Gas".into(),
        Type::Array(t) => format!("[{}]", format_type(t)),
        Type::Map(k, v) => format!("Map<{}, {}>", format_type(k), format_type(v)),
        Type::Option(t) => format!("Option<{}>", format_type(t)),
        Type::Result(t, e) => format!("Result<{}, {}>", format_type(t), format_type(e)),
        Type::Arena(size) => format!("Arena<{}>", size),
        Type::Named(n) => n.clone(),
    }
}

fn type_is_fixed_size(ty: &Type) -> bool {
    match ty {
        Type::Array(_) | Type::Map(_, _) | Type::Option(_) | Type::Result(_, _) | Type::Arena(_) => false,
        _ => true,
    }
}

fn check_expr(expr: &Expr, env: &mut TypeEnv, location: &str) {
    match expr {
        Expr::Alloc { arena, value } => {
            check_expr(arena, env, location);
            check_expr(value, env, location);

            let arena_ty = infer_expr_type(arena, env);
            match arena_ty {
                Some(Type::Arena(_)) => {
                    if let Some(value_ty) = infer_expr_type(value, env) {
                        if !type_is_fixed_size(&value_ty) {
                            env.errors.push(TypeError {
                                message: format!(
                                    "Cannot allocate dynamic type '{}' on Arena",
                                    format_type(&value_ty)
                                ),
                                location: location.to_string(),
                            });
                        }
                    }
                }
                Some(other) => {
                    env.errors.push(TypeError {
                        message: format!(
                            "Allocation source must be an Arena, got '{}'",
                            format_type(&other)
                        ),
                        location: location.to_string(),
                    });
                }
                None => {
                    env.errors.push(TypeError {
                        message: "Allocation source must be an Arena".to_string(),
                        location: location.to_string(),
                    });
                }
            }
        }
        Expr::BinOp { left, right, .. } => {
            check_expr(left, env, location);
            check_expr(right, env, location);
        }
        Expr::UnaryOp { operand, .. } => {
            check_expr(operand, env, location);
        }
        Expr::FnCall { args, .. } => {
            for arg in args {
                check_expr(arg, env, location);
            }
        }
        Expr::MethodCall { object, args, .. } => {
            check_expr(object, env, location);
            for arg in args {
                check_expr(arg, env, location);
            }
        }
        Expr::FieldAccess { object, .. } => {
            check_expr(object, env, location);
        }
        Expr::Index { object, index } => {
            check_expr(object, env, location);
            check_expr(index, env, location);
        }
        Expr::If { condition, then_block, else_block } => {
            check_expr(condition, env, location);
            check_block_exprs(then_block, env, location);
            if let Some(eb) = else_block {
                check_block_exprs(eb, env, location);
            }
        }
        Expr::Block(block) => {
            check_block_exprs(block, env, location);
        }
        _ => {}
    }
}

fn check_block_exprs(block: &Block, env: &mut TypeEnv, location: &str) {
    for stmt in &block.stmts {
        match stmt {
            Stmt::Let { value, .. } | Stmt::Assign { value, .. } | Stmt::Ghost { value, .. } => {
                check_expr(value, env, location);
            }
            Stmt::Return(Some(expr)) | Stmt::Assert { condition: expr, .. } => {
                check_expr(expr, env, location);
            }
            Stmt::If { condition, then_block, else_block } => {
                check_expr(condition, env, location);
                check_block_exprs(then_block, env, location);
                if let Some(eb) = else_block {
                    check_block_exprs(eb, env, location);
                }
            }
            Stmt::While { condition, body, .. } => {
                check_expr(condition, env, location);
                check_block_exprs(body, env, location);
            }
            Stmt::Emit { args, .. } => {
                for arg in args {
                    check_expr(arg, env, location);
                }
            }
            Stmt::Expr(expr) => {
                check_expr(expr, env, location);
            }
            _ => {}
        }
    }
}

// --- Pretty printing ---

pub fn print_type_report(env: &TypeEnv) {
    if !env.errors.is_empty() {
        eprintln!();
        eprintln!(
            "{}",
            "╔══════════════════════════════════════════════════╗".bright_red()
        );
        eprintln!(
            "{}",
            "║         ANVIL TYPE ERRORS                        ║".bright_red()
        );
        eprintln!(
            "{}",
            "╚══════════════════════════════════════════════════╝".bright_red()
        );
        eprintln!();
        for err in &env.errors {
            eprintln!(
                "  {} [{}] {}",
                "✗".bright_red().bold(),
                err.location,
                err.message
            );
        }
    }

    if !env.warnings.is_empty() {
        eprintln!();
        for warn in &env.warnings {
            eprintln!(
                "  {} [{}] {}",
                "⚠".bright_yellow(),
                warn.location,
                warn.message
            );
        }
    }

    if !env.constraints.is_empty() && env.errors.is_empty() {
        eprintln!(
            "  {} {} type constraints registered for Z3",
            "✓".bright_green(),
            env.constraints.len()
        );
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
        assert!(
            env.errors.is_empty(),
            "Unexpected type errors: {:?}",
            env.errors
        );
        // Should have constraints for sender_balance, receiver_balance, amount (2 each: non-neg + upper bound)
        assert!(
            env.constraints.len() >= 6,
            "Expected >= 6 constraints, got {}",
            env.constraints.len()
        );
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
