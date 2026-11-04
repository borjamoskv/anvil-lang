use std::collections::{HashMap, HashSet};
use crate::ast::{Type, Stmt, Expr};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceState {
    Uninitialized,
    Active(Type),
    Moved,
    Dropped,
}

#[derive(Debug)]
pub struct BorrowContext {
    /// Pila de ámbitos léxicos (Markov Blankets)
    pub scopes: Vec<HashMap<String, ResourceState>>,
    /// Manta de Markov: determina si estamos en modo de anergía controlada.
    pub is_unsafe: bool,
}

impl BorrowContext {
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            is_unsafe: false,
        }
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn pop_scope(&mut self) -> HashMap<String, ResourceState> {
        self.scopes.pop().unwrap()
    }

    pub fn declare_var(&mut self, name: String, ty: Type) {
        self.scopes.last_mut().unwrap().insert(name, ResourceState::Active(ty));
    }

    pub fn get_mut_state(&mut self, name: &str) -> Option<&mut ResourceState> {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(state) = scope.get_mut(name) {
                return Some(state);
            }
        }
        None
    }

    pub fn consume_var(&mut self, name: &str) -> Result<Type, String> {
        let state_opt = self.get_mut_state(name);
        match state_opt {
            Some(state) => {
                match state {
                    ResourceState::Active(ty) => {
                        if ty.is_affine() {
                            let extracted_type = ty.clone();
                            *state = ResourceState::Moved;
                            Ok(extracted_type)
                        } else {
                            Ok(ty.clone())
                        }
                    }
                    ResourceState::Moved => Err(format!("Falsación de invariante: Uso después de Move en variable '{}'", name)),
                    ResourceState::Dropped => Err(format!("Falsación de invariante: Uso después de Drop en variable '{}'", name)),
                    ResourceState::Uninitialized => Err(format!("Uso de variable no inicializada '{}'", name)),
                }
            }
            None => Err(format!("Variable no declarada '{}'", name)),
        }
    }

    pub fn synthesize_drops_for_scope(&mut self, mut scope: HashMap<String, ResourceState>) -> Vec<Stmt> {
        let mut drops = Vec::new();
        for (name, state) in scope.iter_mut() {
            if let ResourceState::Active(ty) = state {
                if ty.is_affine() {
                    drops.push(Stmt::SyntheticDrop(name.clone()));
                    *state = ResourceState::Dropped;
                }
            }
        }
        drops
    }

    /// Comprueba si una variable específica sigue activa y es afín para ejecutar un Early Drop (NLL).
    pub fn try_early_drop(&mut self, name: &str) -> Option<Stmt> {
        if let Some(state) = self.get_mut_state(name) {
            if let ResourceState::Active(ty) = state {
                if ty.is_affine() {
                    *state = ResourceState::Dropped;
                    return Some(Stmt::SyntheticDrop(name.to_string()));
                }
            }
        }
        None
    }
}

// --- Geodésicas de No-Léxicas (NLL Liveness Analysis) ---

fn collect_used_vars_expr(expr: &Expr, vars: &mut HashSet<String>) {
    match expr {
        Expr::Variable(name) | Expr::Move(name) | Expr::Borrow(name) | Expr::BorrowMut(name) => {
            vars.insert(name.clone());
        }
        Expr::Deref(inner) => collect_used_vars_expr(inner, vars),
        Expr::Add(l, r) => {
            collect_used_vars_expr(l, vars);
            collect_used_vars_expr(r, vars);
        }
        _ => {}
    }
}

fn collect_used_vars_stmt(stmt: &Stmt, vars: &mut HashSet<String>) {
    match stmt {
        Stmt::Let(_, _, expr) => collect_used_vars_expr(expr, vars),
        Stmt::Assign(left, right) => {
            collect_used_vars_expr(left, vars);
            collect_used_vars_expr(right, vars);
        }
        Stmt::Return(expr) => collect_used_vars_expr(expr, vars),
        Stmt::Block(inner) | Stmt::UnsafeBlock(inner) => {
            for s in inner {
                collect_used_vars_stmt(s, vars);
            }
        }
        Stmt::If { cond, then_branch, else_branch } => {
            collect_used_vars_expr(cond, vars);
            collect_used_vars_stmt(then_branch, vars);
            if let Some(else_b) = else_branch {
                collect_used_vars_stmt(else_b, vars);
            }
        }
        _ => {}
    }
}

/// Calcula el índice de la última sentencia en la que cada variable es observada (Liveness Horizon).
fn compute_liveness_horizon(stmts: &[Stmt]) -> HashMap<String, usize> {
    let mut horizon: HashMap<String, usize> = HashMap::new();
    for (idx, stmt) in stmts.iter().enumerate() {
        if let Stmt::Let(name, _, _) = stmt {
            horizon.entry(name.clone()).or_insert(idx);
        }
        let mut used = HashSet::new();
        collect_used_vars_stmt(stmt, &mut used);
        for var_name in used {
            horizon.insert(var_name, idx);
        }
    }
    horizon
}

pub fn check_statement(stmt: &Stmt, ctx: &mut BorrowContext) -> Result<Vec<Stmt>, String> {
    match stmt {
        Stmt::Let(name, ty, expr) => {
            check_expression(expr, ctx)?;
            
            if let Type::RawPtr(_) = ty {
                if !ctx.is_unsafe {
                    return Err(format!("Anergía detectada: Imposible vincular puntero físico '{}' fuera de una escotilla 'unsafe'", name));
                }
            }
            
            ctx.declare_var(name.clone(), ty.clone());
            Ok(vec![stmt.clone()])
        }
        Stmt::Block(stmts) => {
            ctx.push_scope();
            let liveness = compute_liveness_horizon(stmts);
            let mut transformed_stmts = Vec::new();

            for (idx, s) in stmts.iter().enumerate() {
                transformed_stmts.extend(check_statement(s, ctx)?);

                // NLL (Non-Lexical Lifetimes): Inyección inmediata tras el horizonte geodésico
                for (var_name, &last_idx) in &liveness {
                    if last_idx == idx {
                        if let Some(drop_node) = ctx.try_early_drop(var_name) {
                            transformed_stmts.push(drop_node);
                        }
                    }
                }
            }

            let scope_vars = ctx.pop_scope();
            // Cualquier recurso remanente que no haya caído por NLL se destruye aquí
            let fallback_drops = ctx.synthesize_drops_for_scope(scope_vars);
            transformed_stmts.extend(fallback_drops);
            
            Ok(vec![Stmt::Block(transformed_stmts)])
        }
        Stmt::Assign(left, right) => {
            check_expression(right, ctx)?;
            if let Expr::Deref(inner) = left {
                if let Expr::Variable(name) = &**inner {
                    let state = ctx.get_mut_state(name)
                        .ok_or_else(|| format!("Variable no declarada '{}'", name))?;
                    if let ResourceState::Active(ty) = state {
                        if let Type::RawPtr(_) = ty {
                            if !ctx.is_unsafe {
                                return Err(format!("Anergía detectada: Imposible desreferenciar puntero físico '{}' fuera de 'unsafe'", name));
                            }
                        }
                    }
                }
            }
            Ok(vec![stmt.clone()])
        }
        Stmt::UnsafeBlock(stmts) => {
            ctx.push_scope();
            let previous_unsafe = ctx.is_unsafe;
            ctx.is_unsafe = true; // Elevación epistémica
            
            let liveness = compute_liveness_horizon(stmts);
            let mut transformed_stmts = Vec::new();
            for (idx, s) in stmts.iter().enumerate() {
                transformed_stmts.extend(check_statement(s, ctx)?);
                for (var_name, &last_idx) in &liveness {
                    if last_idx == idx {
                        if let Some(drop_node) = ctx.try_early_drop(var_name) {
                            transformed_stmts.push(drop_node);
                        }
                    }
                }
            }
            
            ctx.is_unsafe = previous_unsafe; // Restauración de Manta de Markov
            
            let scope_vars = ctx.pop_scope();
            let fallback_drops = ctx.synthesize_drops_for_scope(scope_vars);
            transformed_stmts.extend(fallback_drops);
            
            Ok(vec![Stmt::UnsafeBlock(transformed_stmts)])
        }
        Stmt::If { cond, then_branch, else_branch } => {
            check_expression(cond, ctx)?;

            let before_scopes = ctx.scopes.clone();

            let mut then_ctx = BorrowContext {
                scopes: before_scopes.clone(),
                is_unsafe: ctx.is_unsafe,
            };
            let transformed_then = check_statement(then_branch, &mut then_ctx)?;

            let (transformed_else, else_scopes) = if let Some(else_b) = else_branch {
                let mut else_ctx = BorrowContext {
                    scopes: before_scopes.clone(),
                    is_unsafe: ctx.is_unsafe,
                };
                let stmts = check_statement(else_b, &mut else_ctx)?;
                let single_stmt = if stmts.len() == 1 {
                    stmts.into_iter().next().unwrap()
                } else {
                    Stmt::Block(stmts)
                };
                (Some(Box::new(single_stmt)), else_ctx.scopes)
            } else {
                (None, before_scopes.clone())
            };

            // Regla Q1 (INV_C5_QTT_SOUNDNESS):
            // Verificación estricta de paridad de consumo en bifurcaciones condicionales.
            // Si una variable afín se consume en una rama pero no en la otra, falla cerrado.
            for i in 0..before_scopes.len() {
                for (name, initial_state) in &before_scopes[i] {
                    let state_then = then_ctx.scopes[i].get(name).unwrap_or(initial_state);
                    let state_else = else_scopes[i].get(name).unwrap_or(initial_state);

                    if state_then != state_else {
                        if let ResourceState::Active(ty) = initial_state {
                            if ty.is_affine() {
                                return Err(format!(
                                    "LinearVariableMismatch: Consumo asimétrico de recurso afín '{}' en ramas condicionales (Violación Regla Q1 / INV_C5_QTT_SOUNDNESS)",
                                    name
                                ));
                            }
                        }
                    }

                    // Sincronizar estado resultante si ambas ramas tuvieron consumo simétrico
                    if let Some(state) = ctx.scopes[i].get_mut(name) {
                        if state_then == state_else {
                            *state = state_then.clone();
                        }
                    }
                }
            }

            let single_then = if transformed_then.len() == 1 {
                transformed_then.into_iter().next().unwrap()
            } else {
                Stmt::Block(transformed_then)
            };

            Ok(vec![Stmt::If {
                cond: cond.clone(),
                then_branch: Box::new(single_then),
                else_branch: transformed_else,
            }])
        }
        _ => Ok(vec![stmt.clone()]),
    }
}

pub fn check_expression(expr: &Expr, ctx: &mut BorrowContext) -> Result<Type, String> {
    match expr {
        Expr::LiteralInt(_) => Ok(Type::U64),
        Expr::LiteralBool(_) => Ok(Type::Bool),
        Expr::Variable(name) => {
            ctx.consume_var(name)
        }
        Expr::Move(name) => {
            ctx.consume_var(name)
        }
        _ => Err("Expresión no soportada aún".to_string()),
    }
}
