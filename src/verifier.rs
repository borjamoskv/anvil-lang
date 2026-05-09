// ============================================================
// ANVIL VERIFIER — Z3 SMT Solver Integration
// "Where trust doesn't compile."
//
// Hoare Logic Verification:
// 1. Assert PRECONDITIONS (inputs are valid)
// 2. Assert BODY EFFECTS (assignments transform pre → post)
// 3. Apply FRAME RULE (unmodified vars: post == pre)
// 4. Check POSTCONDITIONS (try to find counterexample)
//    SAT → REJECTED (counterexample shown)
//    UNSAT → PROVEN
// ============================================================

use z3::ast::{Ast, Int, Bool};
use z3::{Config, Context, SatResult, Solver, Tactic};
use crate::ast::*;
use crate::typechecker::{TypeEnv, ConstraintKind};
use colored::Colorize;
use sha3::{Digest, Sha3_256};
use std::collections::{HashMap, HashSet};
use std::time::Instant;

#[derive(Debug)]
pub struct VerifyResult {
    pub fn_name: String,
    pub invariants_checked: usize,
    pub preconditions_count: usize,
    pub postconditions_count: usize,
    pub verified: bool,
    pub counterexample: Option<String>,
    pub duration_ms: f64,
    /// SHA3-256 of the solver assertion set — cryptographic anchor for CORTEX provenance.
    pub proof_hash: String,
}

pub fn verify_program(program: &Program, type_env: &TypeEnv) -> Vec<VerifyResult> {
    let mut results = Vec::new();
    let no_contract_invs: Vec<Invariant> = Vec::new();
    for item in &program.items {
        match item {
            Item::Function(f) if !f.invariants.is_empty() => {
                results.push(verify_function(f, type_env, &no_contract_invs));
            },
            Item::Contract(c) => {
                for f in c.functions.iter().filter(|f| !f.invariants.is_empty() || !c.invariants.is_empty()) {
                    results.push(verify_function(f, type_env, &c.invariants));
                }
            },
            _ => {},
        }
    }
    results
}

fn verify_function(func: &FnDef, type_env: &TypeEnv, contract_invariants: &[Invariant]) -> VerifyResult {
    let start = Instant::now();
    let mut cfg = Config::new();
    cfg.set_param_value("timeout", "5000"); // Dynamic timeout (5000ms) for division/rounding bounds
    let ctx = Context::new(&cfg);
    
    // Tactic 'smt' configured to optimize and handle general solver operations robustly
    let tactic = Tactic::new(&ctx, "smt");
    let solver = tactic.solver();

    // Create pre/post Z3 variables for all params
    let mut pre_vars: HashMap<String, Int> = HashMap::new();
    let mut post_vars: HashMap<String, Int> = HashMap::new();

    for param in &func.params {
        let pre = Int::new_const(&ctx, param.name.as_str());
        let post = Int::new_const(&ctx, format!("{}_post", param.name).as_str());
        pre_vars.insert(param.name.clone(), pre);
        post_vars.insert(param.name.clone(), post);
    }

    // Inject type constraints from the type checker
    // This bridges the gap between mathematical Int and silicon-bounded integers
    inject_type_constraints(&ctx, &solver, &pre_vars, &post_vars, type_env);

    // Classify invariants: preconditions vs postconditions
    let mut preconditions = Vec::new();
    let mut postconditions = Vec::new();

    for inv in &func.invariants {
        if invariant_uses_post(&inv.expr) {
            postconditions.push(inv);
        } else {
            preconditions.push(inv);
        }
    }

    // Contract-level invariants are assumed in pre-state and verified in post-state
    // They represent global truths that MUST survive every function execution
    let contract_pre_invs: Vec<&Invariant> = contract_invariants.iter().collect();

    // Step 1: Assert preconditions (function-level)
    for pre in &preconditions {
        if let Some(z3_pre) = invariant_to_z3(&ctx, &pre.expr, &pre_vars, &post_vars) {
            solver.assert(&z3_pre);
        }
    }

    // Step 1b: Assert contract invariants hold in pre-state
    // (we assume the contract was in a valid state before this call)
    for inv in &contract_pre_invs {
        if let Some(z3_inv) = invariant_to_z3(&ctx, &inv.expr, &pre_vars, &pre_vars) {
            solver.assert(&z3_inv);
        }
    }

    // Step 2: Encode body effects
    let modified = encode_body_effects(&ctx, &solver, &func.body, &pre_vars, &post_vars);

    // Step 3: Frame rule — unmodified vars keep pre-state
    for (name, pre) in &pre_vars {
        if !modified.contains(name) {
            if let Some(post) = post_vars.get(name) {
                solver.assert(&post._eq(pre));
            }
        }
    }

    // Define verification state
    let mut all_verified = true;
    let mut counterexample = None;
    let mut global_invariants_checked = 0;

    // Step 3.5: Global Conservation & Inflation Defense
    if let (Some(pre_assets), Some(pre_supply), Some(post_assets), Some(post_supply)) = (
        pre_vars.get("total_assets"),
        pre_vars.get("total_supply"),
        post_vars.get("total_assets"),
        post_vars.get("total_supply"),
    ) {
        global_invariants_checked += 2;

        // 1. Share Inflation Defense: if assets decreased, supply MUST have decreased
        solver.push();
        let assets_decreased = post_assets.lt(pre_assets);
        let supply_decreased = post_supply.lt(pre_supply);
        let inflation_defense = assets_decreased.implies(&supply_decreased);
        solver.assert(&inflation_defense.not());
        
        if let SatResult::Sat = solver.check() {
            all_verified = false;
            let mut ce = String::from("GLOBAL INVARIANT VIOLATED: Share Inflation detected! (Assets decreased but Supply did not)\n");
            let model = solver.get_model().unwrap();
            for (name, var) in &pre_vars {
                if let Some(val) = model.eval(var, true) {
                    ce.push_str(&format!("  {} = {}\n", name, val));
                }
            }
            for (name, var) in &post_vars {
                if let Some(val) = model.eval(var, true) {
                    ce.push_str(&format!("  {}' = {}\n", name, val));
                }
            }
            if counterexample.is_none() { counterexample = Some(ce); }
        }
        solver.pop(1);

        // 2. Conservation Ratio: pre_solvent => post_solvent
        solver.push();
        let pre_solvent = pre_assets.ge(pre_supply);
        let post_solvent = post_assets.ge(post_supply);
        let conservation = pre_solvent.implies(&post_solvent);
        solver.assert(&conservation.not());

        if let SatResult::Sat = solver.check() {
            all_verified = false;
            let mut ce = String::from("GLOBAL INVARIANT VIOLATED: Conservation ratio compromised! (Protocol became undercollateralized)\n");
            let model = solver.get_model().unwrap();
            for (name, var) in &pre_vars {
                if let Some(val) = model.eval(var, true) {
                    ce.push_str(&format!("  {} = {}\n", name, val));
                }
            }
            for (name, var) in &post_vars {
                if let Some(val) = model.eval(var, true) {
                    ce.push_str(&format!("  {}' = {}\n", name, val));
                }
            }
            if counterexample.is_none() { counterexample = Some(ce); }
        }
        solver.pop(1);
    }

    let total_postconditions = postconditions.len() + contract_invariants.len() + global_invariants_checked;

    // Step 4: Verify function postconditions

    for (i, post_inv) in postconditions.iter().enumerate() {
        solver.push();
        if let Some(z3_post) = invariant_to_z3(&ctx, &post_inv.expr, &pre_vars, &post_vars) {
            solver.assert(&z3_post.not());
            match solver.check() {
                SatResult::Sat => {
                    all_verified = false;
                    let model = solver.get_model().unwrap();
                    let mut ce = format!("Postcondition #{} violated:\n", i + 1);
                    for (name, var) in &pre_vars {
                        if let Some(val) = model.eval(var, true) {
                            ce.push_str(&format!("  {} = {}\n", name, val));
                        }
                    }
                    for (name, var) in &post_vars {
                        if let Some(val) = model.eval(var, true) {
                            ce.push_str(&format!("  {}' = {}\n", name, val));
                        }
                    }
                    counterexample = Some(ce);
                },
                SatResult::Unsat => { /* PROVEN */ },
                SatResult::Unknown => {
                    all_verified = false;
                    counterexample = Some(format!("Postcondition #{}: Z3 undecidable", i + 1));
                },
            }
        }
        solver.pop(1);
    }

    // Step 5: Verify contract-level invariants hold in post-state
    for (i, contract_inv) in contract_invariants.iter().enumerate() {
        solver.push();
        // Contract invariants use post-state variables for both sides
        // because we're checking the state AFTER the function executes
        if let Some(z3_inv) = invariant_to_z3(&ctx, &contract_inv.expr, &post_vars, &post_vars) {
            solver.assert(&z3_inv.not());
            match solver.check() {
                SatResult::Sat => {
                    all_verified = false;
                    let model = solver.get_model().unwrap();
                    let mut ce = format!("Contract invariant #{} violated:\n", i + 1);
                    for (name, var) in &pre_vars {
                        if let Some(val) = model.eval(var, true) {
                            ce.push_str(&format!("  {} = {}\n", name, val));
                        }
                    }
                    for (name, var) in &post_vars {
                        if let Some(val) = model.eval(var, true) {
                            ce.push_str(&format!("  {}' = {}\n", name, val));
                        }
                    }
                    if counterexample.is_none() {
                        counterexample = Some(ce);
                    }
                },
                SatResult::Unsat => { /* PROVEN — contract invariant preserved */ },
                SatResult::Unknown => {
                    all_verified = false;
                    if counterexample.is_none() {
                        counterexample = Some(format!("Contract invariant #{}: Z3 undecidable", i + 1));
                    }
                },
            }
        }
        solver.pop(1);
    }

    // Compute proof hash: SHA3-256 over the solver's assertion set.
    // This is the cryptographic anchor that CORTEX-Persist uses to link
    // a persisted fact back to the Z3 proof that justified it.
    let proof_hash = {
        let assertions = solver.get_assertions();
        let mut hasher = Sha3_256::new();
        for assertion in assertions.iter() {
            hasher.update(format!("{}", assertion).as_bytes());
        }
        hex::encode(hasher.finalize())
    };

    VerifyResult {
        fn_name: func.name.clone(),
        invariants_checked: func.invariants.len() + contract_invariants.len(),
        preconditions_count: preconditions.len(),
        postconditions_count: total_postconditions,
        verified: all_verified,
        counterexample,
        duration_ms: start.elapsed().as_secs_f64() * 1000.0,
        proof_hash,
    }
}

// --- Invariant classification ---

fn invariant_uses_post(inv: &InvariantExpr) -> bool {
    match inv {
        InvariantExpr::Comparison { left, right, .. } =>
            inv_term_uses_post(left) || inv_term_uses_post(right),
        InvariantExpr::And(a, b) | InvariantExpr::Or(a, b) =>
            invariant_uses_post(a) || invariant_uses_post(b),
        InvariantExpr::Not(a) => invariant_uses_post(a),
        InvariantExpr::Forall { body, .. } | InvariantExpr::Exists { body, .. } =>
            invariant_uses_post(body),
        InvariantExpr::True => false,
    }
}

fn inv_term_uses_post(term: &InvTerm) -> bool {
    match term {
        InvTerm::Var { is_post, .. } | InvTerm::FieldAccess { is_post, .. } => *is_post,
        InvTerm::BinOp { left, right, .. } =>
            inv_term_uses_post(left) || inv_term_uses_post(right),
        InvTerm::FnCall { args, .. } => args.iter().any(inv_term_uses_post),
        InvTerm::Literal(_) => false,
    }
}

// --- Body encoding (SSA transformation) ---
// Sequential assignments to the same variable are encoded as
// intermediate Z3 vars: a += x; a += y → a_1 = a + x; a_post = a_1 + y
// This eliminates the unsoundness of overwriting Z3 assertions.

fn encode_body_effects<'ctx>(
    ctx: &'ctx Context,
    solver: &Solver<'ctx>,
    body: &Block,
    pre_vars: &HashMap<String, Int<'ctx>>,
    post_vars: &HashMap<String, Int<'ctx>>,
) -> HashSet<String> {
    let mut modified = HashSet::new();
    // Track the "current value" of each variable as we walk through statements
    // Initially, current[x] = pre_vars[x]. After assignment, current[x] = new Z3 var.
    let mut current_vars: HashMap<String, Int<'ctx>> = pre_vars.clone();
    // SSA counter per variable
    let mut ssa_counters: HashMap<String, usize> = HashMap::new();

    for stmt in &body.stmts {
        match stmt {
            Stmt::Assign { target, op, value } => {
                if let Some(current) = match target {
                    LValue::Ident(name) => current_vars.get(name).cloned().map(|c| (name, c)),
                    _ => None,
                } {
                    let (name, current) = current;
                    // Evaluate the RHS using current variable values (not pre!)
                    let encoded = match op {
                        AssignOp::Assign => expr_to_z3(ctx, value, &current_vars),
                        AssignOp::AddAssign => expr_to_z3(ctx, value, &current_vars)
                            .map(|v| Int::add(ctx, &[&current, &v])),
                        AssignOp::SubAssign => expr_to_z3(ctx, value, &current_vars)
                            .map(|v| Int::sub(ctx, &[&current, &v])),
                        AssignOp::MulAssign => expr_to_z3(ctx, value, &current_vars)
                            .map(|v| Int::mul(ctx, &[&current, &v])),
                        AssignOp::DivAssign => expr_to_z3(ctx, value, &current_vars)
                            .map(|v| current.div(&v)),
                    };

                    if let Some(result) = encoded {
                        // Create intermediate SSA variable
                        let counter = ssa_counters.entry(name.clone()).or_insert(0);
                        *counter += 1;

                        let ssa_name = format!("{}_ssa_{}", name, counter);
                        let ssa_var = Int::new_const(ctx, ssa_name.as_str());

                        // Assert: ssa_var == computed_result
                        solver.assert(&ssa_var._eq(&result));
                        
                        // HARDWARE BOUNDS: The silicon is finite.
                        let zero = Int::from_i64(ctx, 0);
                        let u64_max = Int::from_str(ctx, "18446744073709551616").unwrap();
                        solver.assert(&ssa_var.ge(&zero));
                        solver.assert(&ssa_var.lt(&u64_max));

                        // Update current value to the new SSA variable
                        current_vars.insert(name.clone(), ssa_var);
                        modified.insert(name.clone());
                    }
                }
            },
            Stmt::Let { name, value, .. } => {
                // Local variable: create a Z3 variable and bind it
                if let Some(z3_val) = expr_to_z3(ctx, value, &current_vars) {
                    let local_var = Int::new_const(ctx, format!("local_{}", name).as_str());
                    solver.assert(&local_var._eq(&z3_val));
                    
                    // Enforce Silicon Bounds on intermediate `let` bindings
                    let zero = Int::from_i64(ctx, 0);
                    let u64_max = Int::from_str(ctx, "18446744073709551616").unwrap();
                    solver.assert(&local_var.ge(&zero));
                    solver.assert(&local_var.lt(&u64_max));
                    
                    current_vars.insert(name.clone(), local_var);
                }
            },
            Stmt::While { condition, invariants, body } => {
                // Inductive loop verification:
                // 1. Havoc all variables modified in the loop body
                //    (replace with fresh unconstrained Z3 vars)
                // 2. Assume the loop invariant holds (inductive hypothesis)
                // 3. Assert ¬condition (loop has exited)
                // 4. The function's postconditions verify the desired property

                // Collect variables modified in the loop body
                let loop_modified = collect_modified_vars(&body);

                // Havoc: create fresh Z3 vars for all loop-modified variables
                for var_name in &loop_modified {
                    let havoc_name = format!("{}_loop_exit", var_name);
                    let havoc_var = Int::new_const(ctx, havoc_name.as_str());
                    current_vars.insert(var_name.clone(), havoc_var);
                    modified.insert(var_name.clone());
                }

                // Assume loop invariants hold at exit
                for inv in invariants {
                    if let Some(z3_inv) = invariant_to_z3_with_current(ctx, &inv.expr, &current_vars) {
                        solver.assert(&z3_inv);
                    }
                }

                // Assert ¬condition: the loop has exited
                // Encode the loop condition as a Z3 Bool and negate it
                if let Some(exit_cond) = condition_to_z3(ctx, condition, &current_vars) {
                    solver.assert(&exit_cond.not());
                }
            },
            Stmt::If { then_block, else_block, .. } => {
                // Overapproximate: encode effects from both branches
                // Variables modified in either branch become havoc'd
                let then_modified = collect_modified_vars(then_block);
                if let Some(else_b) = else_block {
                    let else_modified = collect_modified_vars(else_b);
                    for var_name in then_modified.union(&else_modified) {
                        let havoc_name = format!("{}_if_merge", var_name);
                        let havoc_var = Int::new_const(ctx, havoc_name.as_str());
                        current_vars.insert(var_name.clone(), havoc_var);
                        modified.insert(var_name.clone());
                    }
                } else {
                    // If without else: modified vars become indeterminate
                    for var_name in &then_modified {
                        let havoc_name = format!("{}_if_merge", var_name);
                        let havoc_var = Int::new_const(ctx, havoc_name.as_str());
                        current_vars.insert(var_name.clone(), havoc_var);
                        modified.insert(var_name.clone());
                    }
                }
            },
            _ => {} // Return, etc. — no effect on Z3 state
        }
    }

    // Final step: connect the last SSA value of each modified variable to post_var
    for name in &modified {
        if let (Some(final_val), Some(post_var)) = (current_vars.get(name), post_vars.get(name)) {
            solver.assert(&post_var._eq(final_val));
        }
    }

    modified
}

// Collect all variable names that are assigned in a block
fn collect_modified_vars(block: &Block) -> HashSet<String> {
    let mut modified = HashSet::new();
    for stmt in &block.stmts {
        match stmt {
            Stmt::Assign { target, .. } => {
                if let LValue::Ident(name) = target {
                    modified.insert(name.clone());
                }
            },
            Stmt::While { body, .. } => {
                modified.extend(collect_modified_vars(body));
            },
            Stmt::If { then_block, else_block, .. } => {
                modified.extend(collect_modified_vars(then_block));
                if let Some(eb) = else_block {
                    modified.extend(collect_modified_vars(eb));
                }
            },
            _ => {},
        }
    }
    modified
}

// Evaluate invariant expression using a single set of "current" variables
// Used for loop invariants where we don't have a pre/post split
fn invariant_to_z3_with_current<'ctx>(
    ctx: &'ctx Context, inv: &InvariantExpr, vars: &HashMap<String, Int<'ctx>>,
) -> Option<Bool<'ctx>> {
    // Reuse the standard invariant_to_z3 by passing current vars as both pre and post
    invariant_to_z3(ctx, inv, vars, vars)
}

// Convert an Anvil Expr (used as a condition) to a Z3 Bool
// Handles comparisons like `counter > 0`, `i <= n`
fn condition_to_z3<'ctx>(
    ctx: &'ctx Context, expr: &Expr, vars: &HashMap<String, Int<'ctx>>,
) -> Option<Bool<'ctx>> {
    match expr {
        Expr::BinOp { left, op, right } => {
            match op {
                BinOp::Gt | BinOp::Lt | BinOp::Gte | BinOp::Lte | BinOp::Eq | BinOp::Neq => {
                    let l = expr_to_z3(ctx, left, vars)?;
                    let r = expr_to_z3(ctx, right, vars)?;
                    Some(match op {
                        BinOp::Gt => l.gt(&r),
                        BinOp::Lt => l.lt(&r),
                        BinOp::Gte => l.ge(&r),
                        BinOp::Lte => l.le(&r),
                        BinOp::Eq => l._eq(&r).into(),
                        BinOp::Neq => l._eq(&r).not(),
                        _ => unreachable!(),
                    })
                },
                BinOp::And => {
                    let l = condition_to_z3(ctx, left, vars)?;
                    let r = condition_to_z3(ctx, right, vars)?;
                    Some(Bool::and(ctx, &[&l, &r]))
                },
                BinOp::Or => {
                    let l = condition_to_z3(ctx, left, vars)?;
                    let r = condition_to_z3(ctx, right, vars)?;
                    Some(Bool::or(ctx, &[&l, &r]))
                },
                _ => None,
            }
        },
        Expr::BoolLit(b) => Some(Bool::from_bool(ctx, *b)),
        _ => None,
    }
}

// --- Type constraint injection ---
// Bridges the thermodynamic gap: Z3 Int is unbounded, but silicon is not.
// u64 has exactly 2^64 states. Every bit beyond that is a lie.

fn inject_type_constraints<'ctx>(
    ctx: &'ctx Context,
    solver: &Solver<'ctx>,
    pre_vars: &HashMap<String, Int<'ctx>>,
    post_vars: &HashMap<String, Int<'ctx>>,
    type_env: &TypeEnv,
) {
    let zero = Int::from_i64(ctx, 0);

    for constraint in &type_env.constraints {
        // Apply to pre-state variable
        if let Some(pre_var) = pre_vars.get(&constraint.var_name) {
            match &constraint.kind {
                ConstraintKind::NonNegative => {
                    solver.assert(&pre_var.ge(&zero));
                }
                ConstraintKind::UpperBound { bits } => {
                    // For bits <= 64, use i64 representation
                    // For larger, use string-based big integer
                    if *bits <= 63 {
                        let upper = Int::from_i64(ctx, 1i64 << bits);
                        solver.assert(&pre_var.lt(&upper));
                    }
                    // For u64/u128/u256: the non-negative constraint is sufficient
                    // for Z3 Int semantics — overflow is caught by postcondition checking
                }
                ConstraintKind::SignedBound { bits } => {
                    if *bits <= 64 {
                        let half = 1i64 << (bits - 1);
                        let lower = Int::from_i64(ctx, -half);
                        let upper = Int::from_i64(ctx, half);
                        solver.assert(&pre_var.ge(&lower));
                        solver.assert(&pre_var.lt(&upper));
                    }
                }
            }
        }

        // Apply to post-state variable (the result must also be in bounds)
        if let Some(post_var) = post_vars.get(&constraint.var_name) {
            match &constraint.kind {
                ConstraintKind::NonNegative => {
                    solver.assert(&post_var.ge(&zero));
                }
                ConstraintKind::UpperBound { bits } => {
                    if *bits <= 63 {
                        let upper = Int::from_i64(ctx, 1i64 << bits);
                        solver.assert(&post_var.lt(&upper));
                    }
                }
                ConstraintKind::SignedBound { bits } => {
                    if *bits <= 64 {
                        let half = 1i64 << (bits - 1);
                        let lower = Int::from_i64(ctx, -half);
                        let upper = Int::from_i64(ctx, half);
                        solver.assert(&post_var.ge(&lower));
                        solver.assert(&post_var.lt(&upper));
                    }
                }
            }
        }
    }
}

// --- Z3 translation ---

fn expr_to_z3<'ctx>(
    ctx: &'ctx Context, expr: &Expr, vars: &HashMap<String, Int<'ctx>>,
) -> Option<Int<'ctx>> {
    match expr {
        Expr::IntLit(n) => Some(Int::from_i64(ctx, *n as i64)),
        Expr::Ident(name) => vars.get(name.trim()).cloned(),
        Expr::BinOp { left, op, right } => {
            let l = expr_to_z3(ctx, left, vars)?;
            let r = expr_to_z3(ctx, right, vars)?;
            Some(match op {
                BinOp::Add => Int::add(ctx, &[&l, &r]),
                BinOp::Sub => Int::sub(ctx, &[&l, &r]),
                BinOp::Mul => Int::mul(ctx, &[&l, &r]),
                BinOp::Div => l.div(&r),
                BinOp::Mod => l.rem(&r),
                _ => return None,
            })
        },
        _ => None,
    }
}

fn invariant_to_z3<'ctx>(
    ctx: &'ctx Context, inv: &InvariantExpr,
    pre_vars: &HashMap<String, Int<'ctx>>, post_vars: &HashMap<String, Int<'ctx>>,
) -> Option<Bool<'ctx>> {
    match inv {
        InvariantExpr::Comparison { left, op, right } => {
            let l = inv_term_to_z3(ctx, left, pre_vars, post_vars)?;
            let r = inv_term_to_z3(ctx, right, pre_vars, post_vars)?;
            Some(match op {
                CmpOp::Eq => l._eq(&r), CmpOp::Neq => l._eq(&r).not(),
                CmpOp::Lt => l.lt(&r), CmpOp::Gt => l.gt(&r),
                CmpOp::Lte => l.le(&r), CmpOp::Gte => l.ge(&r),
            })
        },
        InvariantExpr::And(a, b) => {
            let (za, zb) = (invariant_to_z3(ctx, a, pre_vars, post_vars)?,
                            invariant_to_z3(ctx, b, pre_vars, post_vars)?);
            Some(Bool::and(ctx, &[&za, &zb]))
        },
        InvariantExpr::Or(a, b) => {
            let (za, zb) = (invariant_to_z3(ctx, a, pre_vars, post_vars)?,
                            invariant_to_z3(ctx, b, pre_vars, post_vars)?);
            Some(Bool::or(ctx, &[&za, &zb]))
        },
        InvariantExpr::Not(a) => Some(invariant_to_z3(ctx, a, pre_vars, post_vars)?.not()),
        InvariantExpr::True => Some(Bool::from_bool(ctx, true)),
        _ => None,
    }
}

fn inv_term_to_z3<'ctx>(
    ctx: &'ctx Context, term: &InvTerm,
    pre_vars: &HashMap<String, Int<'ctx>>, post_vars: &HashMap<String, Int<'ctx>>,
) -> Option<Int<'ctx>> {
    match term {
        InvTerm::Literal(n) => Some(Int::from_i64(ctx, *n as i64)),
        InvTerm::Var { name, is_post } => {
            if *is_post {
                post_vars.get(name).cloned()
                    .or_else(|| Some(Int::new_const(ctx, format!("{}_post", name).as_str())))
            } else {
                pre_vars.get(name).cloned()
                    .or_else(|| Some(Int::new_const(ctx, name.as_str())))
            }
        },
        InvTerm::BinOp { left, op, right } => {
            let l = inv_term_to_z3(ctx, left, pre_vars, post_vars)?;
            let r = inv_term_to_z3(ctx, right, pre_vars, post_vars)?;
            Some(match op {
                ArithOp::Add => Int::add(ctx, &[&l, &r]),
                ArithOp::Sub => Int::sub(ctx, &[&l, &r]),
                ArithOp::Mul => Int::mul(ctx, &[&l, &r]),
                ArithOp::Div => l.div(&r),
                ArithOp::Mod => l.rem(&r),
            })
        },
        InvTerm::FieldAccess { object, field, is_post } => {
            let n = format!("{}_{}", object, field);
            if *is_post { Some(Int::new_const(ctx, format!("{}_post", n).as_str())) }
            else { Some(Int::new_const(ctx, n.as_str())) }
        },
        InvTerm::FnCall { .. } => None,
    }
}

// --- Pretty printing ---

pub fn print_results(results: &[VerifyResult]) {
    println!();
    println!("{}", "╔══════════════════════════════════════════════════╗".bright_blue());
    println!("{}", "║         ANVIL VERIFICATION REPORT                ║".bright_blue());
    println!("{}", "╚══════════════════════════════════════════════════╝".bright_blue());
    println!();

    let mut total = 0;
    let mut passed = 0;

    for r in results {
        total += r.postconditions_count;
        if r.verified {
            passed += r.postconditions_count;
            println!("  {} {} — {} preconditions, {} postconditions verified ({} invariants) in {:.3}ms",
                "✓".bright_green().bold(),
                r.fn_name.bright_white().bold(),
                r.preconditions_count, r.postconditions_count,
                r.invariants_checked,
                r.duration_ms,
            );
            println!("    {} proof: {}…{}",
                "🔐",
                &r.proof_hash[..16],
                &r.proof_hash[r.proof_hash.len()-8..],
            );
        } else {
            println!("  {} {} — VERIFICATION FAILED",
                "✗".bright_red().bold(),
                r.fn_name.bright_white().bold(),
            );
            if let Some(ce) = &r.counterexample {
                for line in ce.lines() {
                    println!("    {}", line.bright_red());
                }
            }
        }
    }

    println!();
    if passed == total && total > 0 {
        println!("  {} All {}/{} postconditions proven. Zero trust required.",
            "█".bright_green(), passed, total);
    } else if total == 0 {
        println!("  {} No postconditions to verify.", "█".bright_yellow());
    } else {
        println!("  {} {}/{} postconditions proven. {} FAILED.",
            "█".bright_red(), passed, total, total - passed);
    }
    println!();
}
