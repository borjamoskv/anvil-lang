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

use crate::core::ast::*;
use crate::core::typechecker::{ConstraintKind, TypeConstraint, TypeEnv};
use colored::Colorize;
use sha3::{Digest, Sha3_256};
use std::collections::{HashMap, HashSet};
use std::convert::TryInto;
use std::time::Instant;
use tracing::{info, info_span, warn};
use z3::ast::{Ast, BV, Bool};
use z3::{Config, Context, FuncDecl, SatResult, Solver, Sort, Tactic};

const SOLVER_BV_WIDTH: u32 = 256;
pub const DEFAULT_SOLVER_TIMEOUT_MS: u64 = 5000;

#[derive(Clone, Copy, Debug)]
pub struct VerifyOptions {
    pub timeout_ms: u64,
}

impl Default for VerifyOptions {
    fn default() -> Self {
        Self {
            timeout_ms: DEFAULT_SOLVER_TIMEOUT_MS,
        }
    }
}

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
    /// Non-fatal warnings (e.g., approximated invariants, uninterpreted functions)
    pub warnings: Vec<String>,
}

struct BodyEncoding<'ctx> {
    modified: HashSet<String>,
    current_vars: HashMap<String, BV<'ctx>>,
    var_types: HashMap<String, Type>,
    signed_vars: HashSet<String>,
    failures: Vec<String>,
    fresh_counter: usize,
}

impl<'ctx> BodyEncoding<'ctx> {
    fn new(
        current_vars: HashMap<String, BV<'ctx>>,
        var_types: HashMap<String, Type>,
        signed_vars: HashSet<String>,
    ) -> Self {
        Self {
            modified: HashSet::new(),
            current_vars,
            var_types,
            signed_vars,
            failures: Vec::new(),
            fresh_counter: 0,
        }
    }

    fn fresh_bv(&mut self, ctx: &'ctx Context, prefix: &str) -> BV<'ctx> {
        self.fresh_counter += 1;
        BV::new_const(
            ctx,
            format!("{}_{}", prefix, self.fresh_counter).as_str(),
            SOLVER_BV_WIDTH,
        )
    }

    fn branch_from(&self, current_vars: HashMap<String, BV<'ctx>>) -> Self {
        Self {
            modified: HashSet::new(),
            current_vars,
            var_types: self.var_types.clone(),
            signed_vars: self.signed_vars.clone(),
            failures: Vec::new(),
            fresh_counter: self.fresh_counter,
        }
    }

    fn absorb_branch(&mut self, branch: BodyEncoding<'ctx>) {
        self.fresh_counter = self.fresh_counter.max(branch.fresh_counter);
        self.failures.extend(branch.failures);
    }
}

pub fn verify_program(program: &Program, type_env: &TypeEnv) -> Vec<VerifyResult> {
    verify_program_with_options(program, type_env, VerifyOptions::default())
}

pub fn verify_program_with_options(
    program: &Program,
    type_env: &TypeEnv,
    options: VerifyOptions,
) -> Vec<VerifyResult> {
    let mut results = Vec::new();
    let no_contract_invs: Vec<Invariant> = Vec::new();
    for item in &program.items {
        match item {
            Item::Function(f) if !f.invariants.is_empty() => {
                results.push(verify_function(f, type_env, &[], &no_contract_invs, options));
            }
            Item::Contract(c) => {
                for f in c
                    .functions
                    .iter()
                    .filter(|f| !f.invariants.is_empty() || !c.invariants.is_empty())
                {
                    results.push(verify_function(f, type_env, &c.state_vars, &c.invariants, options));
                }
            }
            _ => {}
        }
    }
    results
}

fn verify_function(
    func: &FnDef,
    _type_env: &TypeEnv,
    state_vars: &[StateVar],
    contract_invariants: &[Invariant],
    options: VerifyOptions,
) -> VerifyResult {
    let _span = info_span!("verify_function", fn_name = %func.name).entered();
    let start = Instant::now();

    // Extract raw preconditions first to find known constants
    let preconditions_raw: Vec<&Invariant> = func.invariants.iter().filter(|inv| !invariant_uses_post(&inv.expr)).collect();
    let constants = collect_known_constants(&preconditions_raw);

    // Simplify the function AST
    let mut simplified_func = func.clone();
    simplified_func.invariants = simplified_func.invariants.into_iter().map(|inv| Invariant {
        expr: simplify_invariant_expr(&inv.expr, &constants),
        span: inv.span,
    }).collect();
    simplified_func.assumes = simplified_func.assumes.into_iter().map(|inv| Invariant {
        expr: simplify_invariant_expr(&inv.expr, &constants),
        span: inv.span,
    }).collect();
    simplified_func.body = simplify_block(&simplified_func.body, &constants);

    // Simplify contract invariants
    let simplified_contract_invariants: Vec<Invariant> = contract_invariants.iter().map(|inv| Invariant {
        expr: simplify_invariant_expr(&inv.expr, &constants),
        span: inv.span.clone(),
    }).collect();

    // Shadow func and contract_invariants so the rest of the function operates on the simplified AST
    let func = &simplified_func;
    let contract_invariants = &simplified_contract_invariants;

    let mut cfg = Config::new();
    cfg.set_timeout_msec(options.timeout_ms);
    let ctx = Context::new(&cfg);

    // Singularity [1/4]: Tensor-SMT Bypass Hook
    let _ = crate::singularity::TensorSMTEngine::default().guide_smt_search("verify_function_ctx");

    // Tactic 'smt' configured to optimize and handle general solver operations robustly
    let tactic = Tactic::new(&ctx, "smt");
    let solver = tactic.solver();

    // Singularity [4/4]: Inyectar topología de Mempool en la verificación matemática
    crate::singularity::MempoolAwareZ3::inject_mempool_topology(&solver, &ctx);

    // Create pre/post Z3 variables for all params
    let mut pre_vars: HashMap<String, BV> = HashMap::new();
    let mut post_vars: HashMap<String, BV> = HashMap::new();
    let mut param_types: HashMap<String, Type> = HashMap::new();

    for param in &func.params {
        let pre = BV::new_const(&ctx, param.name.as_str(), SOLVER_BV_WIDTH);
        let post = BV::new_const(
            &ctx,
            format!("{}_post", param.name).as_str(),
            SOLVER_BV_WIDTH,
        );
        pre_vars.insert(param.name.clone(), pre);
        post_vars.insert(param.name.clone(), post);
        param_types.insert(param.name.clone(), param.ty.clone());
    }

    // Populate state variables
    for var in state_vars {
        let pre = BV::new_const(&ctx, var.name.as_str(), SOLVER_BV_WIDTH);
        let post = BV::new_const(
            &ctx,
            format!("{}_post", var.name).as_str(),
            SOLVER_BV_WIDTH,
        );
        pre_vars.insert(var.name.clone(), pre);
        post_vars.insert(var.name.clone(), post);
        param_types.insert(var.name.clone(), var.ty.clone());
    }

    // Register function return value in post_vars if it returns a non-unit type
    if let Some(ref ret_ty) = func.return_type {
        if *ret_ty != Type::Unit {
            let post = BV::new_const(&ctx, "result_post", SOLVER_BV_WIDTH);
            post_vars.insert("result".to_string(), post);
            param_types.insert("result".to_string(), ret_ty.clone());
        }
    }

    // Inject type constraints from the type checker
    // This bridges the gap between mathematical BV and silicon-bounded integers
    let mut local_constraints = type_constraints_for_function(func);
    for var in state_vars {
        local_constraints.extend(type_constraints_for_binding(&var.name, &var.ty));
    }
    if let Some(ref ret_ty) = func.return_type {
        if *ret_ty != Type::Unit {
            local_constraints.extend(type_constraints_for_binding("result", ret_ty));
        }
    }
    inject_type_constraints(&ctx, &solver, &pre_vars, &post_vars, &local_constraints);
    let signed_vars = signed_vars_from_constraints(&local_constraints);

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

    // Step 0: Assert assumes (environment axioms — trusted, not proven)
    // These are the Frontera Determinista: facts about the external environment
    // that the verifier accepts without proof (e.g., chain state, oracle prices)
    for assumption in &func.assumes {
        match invariant_to_z3(&ctx, &assumption.expr, &pre_vars, &post_vars, &signed_vars) {
            Some(z3_assume) => solver.assert(&z3_assume),
            None => {
                return unverified_result(
                    func,
                    preconditions.len(),
                    postconditions.len() + contract_invariants.len(),
                    start,
                    "Assumption could not be encoded safely",
                );
            }
        }
    }

    // Step 1: Assert preconditions (function-level)
    for pre in &preconditions {
        match invariant_to_z3(&ctx, &pre.expr, &pre_vars, &post_vars, &signed_vars) {
            Some(z3_pre) => solver.assert(&z3_pre),
            None => {
                return unverified_result(
                    func,
                    preconditions.len(),
                    postconditions.len() + contract_invariants.len(),
                    start,
                    "Precondition could not be encoded safely",
                );
            }
        }
    }

    // Step 1b: Assert contract invariants hold in pre-state
    // (we assume the contract was in a valid state before this call)
    for inv in &contract_pre_invs {
        match invariant_to_z3(&ctx, &inv.expr, &pre_vars, &pre_vars, &signed_vars) {
            Some(z3_inv) => solver.assert(&z3_inv),
            None => {
                return unverified_result(
                    func,
                    preconditions.len(),
                    postconditions.len() + contract_invariants.len(),
                    start,
                    "Contract pre-invariant could not be encoded safely",
                );
            }
        }
    }

    let pre_state_unsat = match solver.check() {
        SatResult::Sat => false,
        SatResult::Unsat => {
            return unverified_result(
                func,
                preconditions.len(),
                postconditions.len() + contract_invariants.len(),
                start,
                "Pre-state constraints are inconsistent; vacuous proof rejected",
            );
        }
        SatResult::Unknown => {
            return unverified_result(
                func,
                preconditions.len(),
                postconditions.len() + contract_invariants.len(),
                start,
                "Pre-state constraints are undecidable",
            );
        }
    };

    // Step 2: Encode body effects
    let body_encoding = encode_body_effects(
        &ctx,
        &solver,
        &func.body,
        &pre_vars,
        &post_vars,
        &param_types,
        &signed_vars,
    );

    // Step 3: Frame rule — unmodified vars keep pre-state
    let mut pre_names: Vec<&String> = pre_vars.keys().collect();
    pre_names.sort();
    for name in pre_names {
        if !body_encoding.modified.contains(name) {
            if let (Some(pre), Some(post)) = (pre_vars.get(name), post_vars.get(name)) {
                solver.assert(&post._eq(pre));
            }
        }
    }

    // Define verification state
    let mut all_verified = true;
    let mut counterexample = None;
    let mut global_invariants_checked = 0;
    let mut proof_obligations = Vec::new();

    if !pre_state_unsat && !body_encoding.failures.is_empty() {
        all_verified = false;
        counterexample = Some(format!(
            "Body encoding failed:\n{}",
            body_encoding.failures.join("\n")
        ));
    }

    if !pre_state_unsat {
        match solver.check() {
            SatResult::Sat => {}
            SatResult::Unsat => {
                all_verified = false;
                if counterexample.is_none() {
                    counterexample = Some(
                        "Body/type constraints are inconsistent for a satisfiable pre-state"
                            .to_string(),
                    );
                }
            }
            SatResult::Unknown => {
                all_verified = false;
                if counterexample.is_none() {
                    counterexample =
                        Some("Body/type constraint consistency is undecidable".to_string());
                }
            }
        }
    }

    // Step 3.5: Global Conservation & Inflation Defense
    if let (Some(pre_assets), Some(pre_supply), Some(post_assets), Some(post_supply)) = (
        pre_vars.get("total_assets"),
        pre_vars.get("total_supply"),
        post_vars.get("total_assets"),
        post_vars.get("total_supply"),
    ) {
        global_invariants_checked += 2;
        proof_obligations.push(
            "global:share_inflation_defense:assets_decreased_implies_supply_decreased".to_string(),
        );
        proof_obligations
            .push("global:conservation_ratio:pre_solvent_implies_post_solvent".to_string());

        // 1. Share Inflation Defense: if assets decreased, supply MUST have decreased
        solver.push();
        let assets_decreased = post_assets.bvult(pre_assets);
        let supply_decreased = post_supply.bvult(pre_supply);
        let inflation_defense = assets_decreased.implies(&supply_decreased);
        solver.assert(&inflation_defense.not());

        match solver.check() {
            SatResult::Sat => {
                all_verified = false;
                let mut ce = String::from(
                    "GLOBAL INVARIANT VIOLATED: Share Inflation detected! (Assets decreased but Supply did not)\n",
                );
                let model = solver.get_model().unwrap();
                append_model_values(&mut ce, &model, &pre_vars, "");
                append_model_values(&mut ce, &model, &post_vars, "'");
                if counterexample.is_none() {
                    counterexample = Some(ce);
                }
            }
            SatResult::Unsat => {}
            SatResult::Unknown => {
                all_verified = false;
                if counterexample.is_none() {
                    counterexample = Some(
                        "Global invariant Share Inflation Defense: Z3 undecidable".to_string(),
                    );
                }
            }
        }
        solver.pop(1);

        // 2. Conservation Ratio: pre_solvent => post_solvent
        solver.push();
        let pre_solvent = pre_assets.bvuge(pre_supply);
        let post_solvent = post_assets.bvuge(post_supply);
        let conservation = pre_solvent.implies(&post_solvent);
        solver.assert(&conservation.not());

        match solver.check() {
            SatResult::Sat => {
                all_verified = false;
                let mut ce = String::from(
                    "GLOBAL INVARIANT VIOLATED: Conservation ratio compromised! (Protocol became undercollateralized)\n",
                );
                let model = solver.get_model().unwrap();
                append_model_values(&mut ce, &model, &pre_vars, "");
                append_model_values(&mut ce, &model, &post_vars, "'");
                if counterexample.is_none() {
                    counterexample = Some(ce);
                }
            }
            SatResult::Unsat => {}
            SatResult::Unknown => {
                all_verified = false;
                if counterexample.is_none() {
                    counterexample =
                        Some("Global invariant Conservation Ratio: Z3 undecidable".to_string());
                }
            }
        }
        solver.pop(1);
    }

    let total_postconditions =
        postconditions.len() + contract_invariants.len() + global_invariants_checked;

    // Step 4: Verify function postconditions

    for (i, post_inv) in postconditions.iter().enumerate() {
        solver.push();
        match invariant_to_z3(&ctx, &post_inv.expr, &pre_vars, &post_vars, &signed_vars) {
            Some(z3_post) => {
                proof_obligations.push(format!("postcondition:{}:{:?}", i + 1, post_inv.expr));
                solver.assert(&z3_post.not());
                match solver.check() {
                    SatResult::Sat => {
                        all_verified = false;
                        let model = solver.get_model().unwrap();
                        let mut ce = format!("Postcondition #{} violated:\n", i + 1);
                        append_model_values(&mut ce, &model, &pre_vars, "");
                        append_model_values(&mut ce, &model, &post_vars, "'");
                        counterexample = Some(ce);
                    }
                    SatResult::Unsat => { /* PROVEN */ }
                    SatResult::Unknown => {
                        all_verified = false;
                        counterexample = Some(format!("Postcondition #{}: Z3 undecidable", i + 1));
                    }
                }
            }
            None => {
                all_verified = false;
                counterexample = Some(format!(
                    "Postcondition #{} could not be encoded safely",
                    i + 1
                ));
            }
        }
        solver.pop(1);
    }

    // Step 5: Verify contract-level invariants hold in post-state
    for (i, contract_inv) in contract_invariants.iter().enumerate() {
        solver.push();
        // Contract invariants use post-state variables for both sides
        // because we're checking the state AFTER the function executes
        match invariant_to_z3(
            &ctx,
            &contract_inv.expr,
            &post_vars,
            &post_vars,
            &signed_vars,
        ) {
            Some(z3_inv) => {
                proof_obligations.push(format!(
                    "contract_invariant:{}:{:?}",
                    i + 1,
                    contract_inv.expr
                ));
                solver.assert(&z3_inv.not());
                match solver.check() {
                    SatResult::Sat => {
                        all_verified = false;
                        let model = solver.get_model().unwrap();
                        let mut ce = format!("Contract invariant #{} violated:\n", i + 1);
                        append_model_values(&mut ce, &model, &pre_vars, "");
                        append_model_values(&mut ce, &model, &post_vars, "'");
                        if counterexample.is_none() {
                            counterexample = Some(ce);
                        }
                    }
                    SatResult::Unsat => { /* PROVEN — contract invariant preserved */ }
                    SatResult::Unknown => {
                        all_verified = false;
                        if counterexample.is_none() {
                            counterexample =
                                Some(format!("Contract invariant #{}: Z3 undecidable", i + 1));
                        }
                    }
                }
            }
            None => {
                all_verified = false;
                if counterexample.is_none() {
                    counterexample = Some(format!(
                        "Contract invariant #{} could not be encoded safely",
                        i + 1
                    ));
                }
            }
        }
        solver.pop(1);
    }

    // Compute proof hash: SHA3-256 over the solver's assertion set.
    // This is the cryptographic anchor that CORTEX-Persist uses to link
    // a persisted fact back to the Z3 proof that justified it.
    let proof_hash = {
        let mut assertions: Vec<String> = solver
            .get_assertions()
            .iter()
            .map(|assertion| format!("{}", assertion))
            .collect();
        assertions.sort();
        proof_obligations.sort();

        let mut hasher = Sha3_256::new();
        for assertion in assertions {
            hasher.update(b"assertion:");
            hasher.update(assertion.as_bytes());
            hasher.update([0]);
        }
        for obligation in proof_obligations {
            hasher.update(b"obligation:");
            hasher.update(obligation.as_bytes());
            hasher.update([0]);
        }
        hex::encode(hasher.finalize())
    };

    let result = VerifyResult {
        fn_name: func.name.clone(),
        invariants_checked: func.invariants.len() + contract_invariants.len(),
        preconditions_count: preconditions.len(),
        postconditions_count: total_postconditions,
        verified: all_verified,
        counterexample,
        duration_ms: start.elapsed().as_secs_f64() * 1000.0,
        proof_hash,
        warnings: Vec::new(),
    };

    if result.verified {
        info!(
            fn_name = %result.fn_name,
            duration_ms = result.duration_ms,
            proof_hash = %&result.proof_hash[..16],
            "Function verified"
        );
    } else {
        warn!(
            fn_name = %result.fn_name,
            duration_ms = result.duration_ms,
            "Function verification FAILED"
        );
    }

    result
}

fn unverified_result(
    func: &FnDef,
    preconditions_count: usize,
    postconditions_count: usize,
    start: Instant,
    reason: &str,
) -> VerifyResult {
    let mut hasher = Sha3_256::new();
    hasher.update(func.name.as_bytes());
    hasher.update([0]);
    hasher.update(reason.as_bytes());

    VerifyResult {
        fn_name: func.name.clone(),
        invariants_checked: func.invariants.len(),
        preconditions_count,
        postconditions_count,
        verified: false,
        counterexample: Some(reason.to_string()),
        duration_ms: start.elapsed().as_secs_f64() * 1000.0,
        proof_hash: hex::encode(hasher.finalize()),
        warnings: Vec::new(),
    }
}

fn append_model_values<'ctx>(
    output: &mut String,
    model: &z3::Model<'ctx>,
    vars: &HashMap<String, BV<'ctx>>,
    suffix: &str,
) {
    let mut names: Vec<&String> = vars.keys().collect();
    names.sort();
    for name in names {
        if let Some(var) = vars.get(name) {
            if let Some(val) = model.eval(var, true) {
                output.push_str(&format!("  {}{} = {}\n", name, suffix, val));
            }
        }
    }
}

// --- Invariant classification ---

fn invariant_uses_post(inv: &InvariantExpr) -> bool {
    match inv {
        InvariantExpr::Comparison { left, right, .. } => {
            inv_term_uses_post(left) || inv_term_uses_post(right)
        }
        InvariantExpr::And(a, b) | InvariantExpr::Or(a, b) => {
            invariant_uses_post(a) || invariant_uses_post(b)
        }
        InvariantExpr::Not(a) => invariant_uses_post(a),
        InvariantExpr::Forall { body, .. } | InvariantExpr::Exists { body, .. } => {
            invariant_uses_post(body)
        }
        InvariantExpr::True => false,
    }
}

fn inv_term_uses_post(term: &InvTerm) -> bool {
    match term {
        InvTerm::Var { is_post, .. } | InvTerm::FieldAccess { is_post, .. } => *is_post,
        InvTerm::BinOp { left, right, .. } => inv_term_uses_post(left) || inv_term_uses_post(right),
        InvTerm::FnCall { args, .. } => args.iter().any(inv_term_uses_post),
        InvTerm::Literal(_) | InvTerm::BigLiteral(_) => false,
    }
}

fn signed_vars_from_constraints(constraints: &[TypeConstraint]) -> HashSet<String> {
    constraints
        .iter()
        .filter_map(|constraint| match constraint.kind {
            ConstraintKind::SignedBound { .. } => Some(constraint.var_name.clone()),
            _ => None,
        })
        .collect()
}

fn type_constraints_for_function(func: &FnDef) -> Vec<TypeConstraint> {
    func.params
        .iter()
        .flat_map(|param| type_constraints_for_binding(&param.name, &param.ty))
        .collect()
}

fn type_constraints_for_binding(name: &str, ty: &Type) -> Vec<TypeConstraint> {
    let mut constraints = Vec::new();
    match ty {
        Type::U8 => {
            push_nonnegative_constraint(&mut constraints, name);
            push_upper_constraint(&mut constraints, name, 8);
        }
        Type::U16 => {
            push_nonnegative_constraint(&mut constraints, name);
            push_upper_constraint(&mut constraints, name, 16);
        }
        Type::U32 => {
            push_nonnegative_constraint(&mut constraints, name);
            push_upper_constraint(&mut constraints, name, 32);
        }
        Type::U64 | Type::Gas => {
            push_nonnegative_constraint(&mut constraints, name);
            push_upper_constraint(&mut constraints, name, 64);
        }
        Type::U128 => {
            push_nonnegative_constraint(&mut constraints, name);
            push_upper_constraint(&mut constraints, name, 128);
        }
        Type::U256 | Type::Wallet | Type::TxHash => {
            push_nonnegative_constraint(&mut constraints, name);
            push_upper_constraint(&mut constraints, name, 256);
        }
        Type::I8 => push_signed_constraint(&mut constraints, name, 8),
        Type::I16 => push_signed_constraint(&mut constraints, name, 16),
        Type::I32 => push_signed_constraint(&mut constraints, name, 32),
        Type::I64 => push_signed_constraint(&mut constraints, name, 64),
        Type::I128 => push_signed_constraint(&mut constraints, name, 128),
        _ => {}
    }

    constraints
}

fn push_nonnegative_constraint(constraints: &mut Vec<TypeConstraint>, name: &str) {
    constraints.push(TypeConstraint {
        var_name: name.to_string(),
        kind: ConstraintKind::NonNegative,
    });
}

fn push_upper_constraint(constraints: &mut Vec<TypeConstraint>, name: &str, bits: u32) {
    constraints.push(TypeConstraint {
        var_name: name.to_string(),
        kind: ConstraintKind::UpperBound { bits },
    });
}

fn push_signed_constraint(constraints: &mut Vec<TypeConstraint>, name: &str, bits: u32) {
    constraints.push(TypeConstraint {
        var_name: name.to_string(),
        kind: ConstraintKind::SignedBound { bits },
    });
}

fn type_is_signed(ty: &Type) -> bool {
    matches!(
        ty,
        Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::I128
    )
}

fn type_rank_for_verifier(ty: &Type) -> u32 {
    match ty {
        Type::U8 | Type::I8 => 1,
        Type::U16 | Type::I16 => 2,
        Type::U32 | Type::I32 => 3,
        Type::U64 | Type::I64 => 4,
        Type::U128 | Type::I128 => 5,
        Type::U256 => 6,
        _ => 0,
    }
}

fn promote_types_for_verifier(a: &Type, b: &Type) -> Type {
    if type_rank_for_verifier(a) >= type_rank_for_verifier(b) {
        a.clone()
    } else {
        b.clone()
    }
}

fn infer_negated_expr_type_for_verifier(
    expr: &Expr,
    var_types: &HashMap<String, Type>,
) -> Option<Type> {
    match expr {
        Expr::IntLit(n) if *n >= 0 && *n <= 128 => Some(Type::I8),
        Expr::IntLit(n) if *n >= 0 && *n <= 32768 => Some(Type::I16),
        Expr::IntLit(n) if *n >= 0 && *n <= 2147483648 => Some(Type::I32),
        Expr::IntLit(n) if *n >= 0 && *n <= 9223372036854775808_i128 => Some(Type::I64),
        Expr::IntLit(_) => Some(Type::I128),
        Expr::BigIntLit(n) if decimal_lte(n, I128_MIN_ABS_DEC) => Some(Type::I128),
        Expr::BigIntLit(_) => None,
        _ => infer_expr_type_for_verifier(expr, var_types).filter(type_is_signed),
    }
}

fn infer_expr_type_for_verifier(expr: &Expr, var_types: &HashMap<String, Type>) -> Option<Type> {
    match expr {
        Expr::IntLit(n) => {
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
        Expr::BigIntLit(_) | Expr::HexLit(_) => Some(Type::U256),
        Expr::BoolLit(_) => Some(Type::Bool),
        Expr::StringLit(_) => Some(Type::String),
        Expr::AddressLit(_) => Some(Type::Address),
        Expr::Ident(name) => var_types.get(name).cloned(),
        Expr::BinOp { left, op, right } => match op {
            BinOp::Eq
            | BinOp::Neq
            | BinOp::Lt
            | BinOp::Gt
            | BinOp::Lte
            | BinOp::Gte
            | BinOp::And
            | BinOp::Or => Some(Type::Bool),
            _ => match (
                infer_expr_type_for_verifier(left, var_types),
                infer_expr_type_for_verifier(right, var_types),
            ) {
                (Some(left), Some(right)) => Some(promote_types_for_verifier(&left, &right)),
                (Some(ty), None) | (None, Some(ty)) => Some(ty),
                _ => None,
            },
        },
        Expr::UnaryOp { op, operand } => match op {
            UnaryOp::Not => Some(Type::Bool),
            UnaryOp::Neg => infer_negated_expr_type_for_verifier(operand, var_types),
        },
        _ => None,
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
    pre_vars: &HashMap<String, BV<'ctx>>,
    post_vars: &HashMap<String, BV<'ctx>>,
    param_types: &HashMap<String, Type>,
    signed_vars: &HashSet<String>,
) -> BodyEncoding<'ctx> {
    let mut encoding =
        BodyEncoding::new(pre_vars.clone(), param_types.clone(), signed_vars.clone());
    encode_block(ctx, solver, body, &mut encoding);

    // Final step: connect the last SSA value of each modified variable to post_var.
    // This must happen once at the top level; recursive if/loop encoding must never
    // assert branch-local effects directly against post-state variables.
    let mut modified_names: Vec<&String> = encoding.modified.iter().collect();
    modified_names.sort();
    for name in modified_names {
        if let (Some(final_val), Some(post_var)) =
            (encoding.current_vars.get(name), post_vars.get(name))
        {
            solver.assert(&post_var._eq(final_val));
        }
    }

    encoding
}

fn encode_block<'ctx>(
    ctx: &'ctx Context,
    solver: &Solver<'ctx>,
    body: &Block,
    encoding: &mut BodyEncoding<'ctx>,
) {
    for stmt in &body.stmts {
        match stmt {
            Stmt::Assign { target, op, value } => {
                if let LValue::Ident(name) = target {
                    if let Some(current) = encoding.current_vars.get(name).cloned() {
                        let target_is_signed = encoding.signed_vars.contains(name);
                        // Evaluate the RHS using current variable values (not pre!)
                        let encoded = match op {
                            AssignOp::Assign => expr_to_z3(
                                ctx,
                                value,
                                &encoding.current_vars,
                                &encoding.signed_vars,
                            ),
                            AssignOp::AddAssign => expr_to_z3(
                                ctx,
                                value,
                                &encoding.current_vars,
                                &encoding.signed_vars,
                            )
                            .map(|v| current.bvadd(&v)),
                            AssignOp::SubAssign => expr_to_z3(
                                ctx,
                                value,
                                &encoding.current_vars,
                                &encoding.signed_vars,
                            )
                            .map(|v| current.bvsub(&v)),
                            AssignOp::MulAssign => expr_to_z3(
                                ctx,
                                value,
                                &encoding.current_vars,
                                &encoding.signed_vars,
                            )
                            .map(|v| current.bvmul(&v)),
                            AssignOp::DivAssign => expr_to_z3(
                                ctx,
                                value,
                                &encoding.current_vars,
                                &encoding.signed_vars,
                            )
                            .map(|v| {
                                if target_is_signed
                                    || expr_uses_signed_var(value, &encoding.signed_vars)
                                {
                                    current.bvsdiv(&v)
                                } else {
                                    current.bvudiv(&v)
                                }
                            }),
                            AssignOp::BitAndAssign => expr_to_z3(
                                ctx,
                                value,
                                &encoding.current_vars,
                                &encoding.signed_vars,
                            )
                            .map(|v| current.bvand(&v)),
                            AssignOp::BitOrAssign => expr_to_z3(
                                ctx,
                                value,
                                &encoding.current_vars,
                                &encoding.signed_vars,
                            )
                            .map(|v| current.bvor(&v)),
                            AssignOp::BitXorAssign => expr_to_z3(
                                ctx,
                                value,
                                &encoding.current_vars,
                                &encoding.signed_vars,
                            )
                            .map(|v| current.bvxor(&v)),
                            AssignOp::ShlAssign => expr_to_z3(
                                ctx,
                                value,
                                &encoding.current_vars,
                                &encoding.signed_vars,
                            )
                            .map(|v| current.bvshl(&v)),
                            AssignOp::ShrAssign => expr_to_z3(
                                ctx,
                                value,
                                &encoding.current_vars,
                                &encoding.signed_vars,
                            )
                            .map(|v| {
                                if target_is_signed {
                                    current.bvashr(&v)
                                } else {
                                    current.bvlshr(&v)
                                }
                            }),
                        };

                        if let Some(result) = encoded {
                            let ssa_var = encoding.fresh_bv(ctx, &format!("{}_ssa", name));

                            // Assert: ssa_var == computed_result
                            solver.assert(&ssa_var._eq(&result));

                            // Update current value to the new SSA variable
                            encoding.current_vars.insert(name.clone(), ssa_var);
                            encoding.modified.insert(name.clone());
                        } else {
                            encoding.failures.push(format!(
                                "Assignment to '{}' could not be encoded safely",
                                name
                            ));
                        }
                    } else {
                        encoding.failures.push(format!(
                            "Assignment to unknown variable '{}' could not be encoded",
                            name
                        ));
                    }
                } else {
                    encoding.failures.push(format!(
                        "Assignment target {:?} is not supported by the verifier",
                        target
                    ));
                }
            }
            Stmt::Let {
                name, ty, value, ..
            } => {
                // Local variable: create a Z3 variable and bind it
                if let Some(z3_val) =
                    expr_to_z3(ctx, value, &encoding.current_vars, &encoding.signed_vars)
                {
                    let local_var =
                        BV::new_const(ctx, format!("local_{}", name).as_str(), SOLVER_BV_WIDTH);
                    solver.assert(&local_var._eq(&z3_val));
                    if let Some(binding_ty) = ty
                        .clone()
                        .or_else(|| infer_expr_type_for_verifier(value, &encoding.var_types))
                    {
                        if type_is_signed(&binding_ty) {
                            encoding.signed_vars.insert(name.clone());
                        }
                        inject_constraints_for_var(
                            ctx,
                            solver,
                            &local_var,
                            &type_constraints_for_binding(name, &binding_ty),
                        );
                        encoding.var_types.insert(name.clone(), binding_ty);
                    }

                    encoding.current_vars.insert(name.clone(), local_var);
                } else {
                    encoding
                        .failures
                        .push(format!("let '{}' could not be encoded safely", name));
                }
            }
            Stmt::While {
                condition,
                invariants,
                body,
            } => {
                // Inductive loop verification:
                // 1. Havoc all variables modified in the loop body
                //    (replace with fresh unconstrained Z3 vars)
                // 2. Assume the loop invariant holds (inductive hypothesis)
                // 3. Assert ¬condition (loop has exited)
                // 4. The function's postconditions verify the desired property

                let loop_modified = collect_modified_vars(body);

                // Prove loop invariants are true before entering the loop.
                for (i, inv) in invariants.iter().enumerate() {
                    match invariant_to_z3_with_current(
                        ctx,
                        &inv.expr,
                        &encoding.current_vars,
                        &encoding.signed_vars,
                    ) {
                        Some(z3_inv) => {
                            solver.push();
                            solver.assert(&z3_inv.not());
                            match solver.check() {
                                SatResult::Unsat => {}
                                SatResult::Sat => encoding.failures.push(format!(
                                    "Loop invariant #{} is not established before the loop",
                                    i + 1
                                )),
                                SatResult::Unknown => encoding.failures.push(format!(
                                    "Loop invariant #{} establishment is undecidable",
                                    i + 1
                                )),
                            }
                            solver.pop(1);
                        }
                        None => encoding.failures.push(format!(
                            "Loop invariant #{} could not be encoded safely",
                            i + 1
                        )),
                    }
                }

                if !invariants.is_empty() {
                    let loop_entry_vars = encoding.current_vars.clone();
                    let mut preservation_encoding = encoding.branch_from(loop_entry_vars.clone());

                    solver.push();
                    for inv in invariants {
                        if let Some(z3_inv) = invariant_to_z3_with_current(
                            ctx,
                            &inv.expr,
                            &loop_entry_vars,
                            &encoding.signed_vars,
                        ) {
                            solver.assert(&z3_inv);
                        }
                    }

                    match condition_to_z3(ctx, condition, &loop_entry_vars, &encoding.signed_vars) {
                        Some(loop_cond) => solver.assert(&loop_cond),
                        None => encoding
                            .failures
                            .push("Loop condition could not be encoded safely".to_string()),
                    }

                    encode_block(ctx, solver, body, &mut preservation_encoding);
                    encoding.fresh_counter = encoding
                        .fresh_counter
                        .max(preservation_encoding.fresh_counter);
                    for failure in &preservation_encoding.failures {
                        encoding
                            .failures
                            .push(format!("Loop body preservation failed: {}", failure));
                    }

                    for (i, inv) in invariants.iter().enumerate() {
                        match invariant_to_z3_with_current(
                            ctx,
                            &inv.expr,
                            &preservation_encoding.current_vars,
                            &encoding.signed_vars,
                        ) {
                            Some(z3_inv) => {
                                solver.push();
                                solver.assert(&z3_inv.not());
                                match solver.check() {
                                    SatResult::Unsat => {}
                                    SatResult::Sat => encoding.failures.push(format!(
                                        "Loop invariant #{} is not preserved by the loop body",
                                        i + 1
                                    )),
                                    SatResult::Unknown => encoding.failures.push(format!(
                                        "Loop invariant #{} preservation is undecidable",
                                        i + 1
                                    )),
                                }
                                solver.pop(1);
                            }
                            None => encoding.failures.push(format!(
                                "Loop invariant #{} could not be encoded after the loop body",
                                i + 1
                            )),
                        }
                    }
                    solver.pop(1);
                }

                // Conservative loop summary. We do not assume user-provided loop invariants
                // unless they were established and preserved above; variables not described by
                // invariants remain unconstrained at loop exit.
                // Havoc: create fresh Z3 vars for all loop-modified variables
                let mut loop_modified_names: Vec<&String> = loop_modified.iter().collect();
                loop_modified_names.sort();
                for var_name in loop_modified_names {
                    let havoc_name = format!("{}_loop_exit", var_name);
                    let havoc_var = BV::new_const(ctx, havoc_name.as_str(), SOLVER_BV_WIDTH);
                    encoding.current_vars.insert(var_name.clone(), havoc_var);
                    encoding.modified.insert(var_name.clone());
                }

                // Assert ¬condition: the loop has exited
                // Encode the loop condition as a Z3 Bool and negate it
                if let Some(exit_cond) = condition_to_z3(
                    ctx,
                    condition,
                    &encoding.current_vars,
                    &encoding.signed_vars,
                ) {
                    solver.assert(&exit_cond.not());
                } else {
                    encoding
                        .failures
                        .push("Loop condition could not be encoded safely".to_string());
                }

                for inv in invariants {
                    if let Some(z3_inv) = invariant_to_z3_with_current(
                        ctx,
                        &inv.expr,
                        &encoding.current_vars,
                        &encoding.signed_vars,
                    ) {
                        solver.assert(&z3_inv);
                    }
                }
            }
            Stmt::If {
                condition,
                then_block,
                else_block,
            } => {
                if let Some(cond_z3) = condition_to_z3(
                    ctx,
                    condition,
                    &encoding.current_vars,
                    &encoding.signed_vars,
                ) {
                    let saved_current = encoding.current_vars.clone();

                    // If the condition is statically true/false, only encode the reachable branch.
                    if let Expr::BoolLit(take_then) = condition {
                        let selected = if *take_then {
                            Some(then_block)
                        } else {
                            else_block.as_ref()
                        };

                        if let Some(block) = selected {
                            let mut selected_encoding = encoding.branch_from(saved_current.clone());
                            encode_block(ctx, solver, block, &mut selected_encoding);
                            let selected_modified = selected_encoding.modified.clone();
                            let selected_current = selected_encoding.current_vars.clone();
                            encoding.absorb_branch(selected_encoding);

                            if encoding.failures.is_empty() {
                                encoding.current_vars = saved_current;
                                let mut selected_modified_names: Vec<String> =
                                    selected_modified.into_iter().collect();
                                selected_modified_names.sort();
                                for var_name in selected_modified_names {
                                    if let Some(value) = selected_current.get(&var_name) {
                                        encoding
                                            .current_vars
                                            .insert(var_name.clone(), value.clone());
                                        encoding.modified.insert(var_name);
                                    }
                                }
                            }
                        }
                        continue;
                    }

                    // 1. Encode 'then' branch
                    let mut then_encoding = encoding.branch_from(saved_current.clone());
                    encode_block(ctx, solver, then_block, &mut then_encoding);
                    let then_modified = then_encoding.modified.clone();
                    let then_current = then_encoding.current_vars.clone();

                    // 2. Encode 'else' branch
                    let mut else_encoding = encoding.branch_from(saved_current.clone());
                    else_encoding.fresh_counter = then_encoding.fresh_counter;
                    if let Some(eb) = else_block {
                        encode_block(ctx, solver, eb, &mut else_encoding);
                    }
                    let else_modified = else_encoding.modified.clone();
                    let else_current = else_encoding.current_vars.clone();

                    encoding.absorb_branch(then_encoding);
                    encoding.absorb_branch(else_encoding);

                    if !encoding.failures.is_empty() {
                        encoding.current_vars = saved_current;
                        continue;
                    }

                    // 3. Merge branches using ITE
                    let mut all_modified: Vec<String> =
                        then_modified.union(&else_modified).cloned().collect();
                    all_modified.sort();
                    encoding.current_vars = saved_current.clone(); // Start merge from baseline

                    for var_name in all_modified {
                        let val_then = then_current
                            .get(&var_name)
                            .cloned()
                            .unwrap_or_else(|| saved_current.get(&var_name).cloned().unwrap());
                        let val_else = else_current
                            .get(&var_name)
                            .cloned()
                            .unwrap_or_else(|| saved_current.get(&var_name).cloned().unwrap());

                        let merge_var = encoding.fresh_bv(ctx, &format!("{}_ssa_if", var_name));

                        // merge_var = cond ? val_then : val_else
                        solver.assert(&merge_var._eq(&cond_z3.ite(&val_then, &val_else)));

                        encoding.current_vars.insert(var_name.clone(), merge_var);
                        encoding.modified.insert(var_name.clone());
                    }
                } else {
                    encoding
                        .failures
                        .push("If condition could not be encoded safely".to_string());
                }
            }
            Stmt::Ghost {
                name, ty, value, ..
            } => {
                // Ghost variables exist only in the proof domain.
                // Create a Z3 variable and bind it to the expression value.
                // These are NOT compiled to silicon — they exist for Z3 reasoning only.
                if let Some(z3_val) =
                    expr_to_z3(ctx, value, &encoding.current_vars, &encoding.signed_vars)
                {
                    let ghost_var =
                        BV::new_const(ctx, format!("ghost_{}", name).as_str(), SOLVER_BV_WIDTH);
                    solver.assert(&ghost_var._eq(&z3_val));
                    if type_is_signed(ty) {
                        encoding.signed_vars.insert(name.clone());
                    }
                    inject_constraints_for_var(
                        ctx,
                        solver,
                        &ghost_var,
                        &type_constraints_for_binding(name, ty),
                    );
                    encoding.var_types.insert(name.clone(), ty.clone());
                    encoding.current_vars.insert(name.clone(), ghost_var);
                } else {
                    encoding
                        .failures
                        .push(format!("ghost '{}' could not be encoded safely", name));
                }
            }
            Stmt::Assert { condition, message } => {
                match condition_to_z3(
                    ctx,
                    condition,
                    &encoding.current_vars,
                    &encoding.signed_vars,
                ) {
                    Some(cond) => {
                        solver.push();
                        solver.assert(&cond.not());
                        match solver.check() {
                            SatResult::Unsat => {
                                solver.pop(1);
                                solver.assert(&cond);
                            }
                            SatResult::Sat => {
                                solver.pop(1);
                                let label = message
                                    .as_deref()
                                    .map(|m| format!(": {}", m))
                                    .unwrap_or_default();
                                encoding
                                    .failures
                                    .push(format!("assertion can fail{}", label));
                            }
                            SatResult::Unknown => {
                                solver.pop(1);
                                encoding
                                    .failures
                                    .push("assertion is undecidable".to_string());
                            }
                        }
                    }
                    None => encoding
                        .failures
                        .push("assertion could not be encoded safely".to_string()),
                }
            }
            Stmt::Emit { .. } => {
                // Emit statements have no effect on Z3 verification state.
                // They are on-chain event markers, compiled to LOG opcodes.
            }
            Stmt::Expr(expr) if expr_contains_fncall(expr) => {
                encoding.failures.push(
                    "Function-call expression statements are not modeled by the verifier"
                        .to_string(),
                );
            }
            Stmt::Expr(_) => {
                // Pure expression statements have no effect on the verification state.
            }
            Stmt::Return(Some(expr)) => {
                if let Some(z3_val) = expr_to_z3(ctx, expr, &encoding.current_vars, &encoding.signed_vars) {
                    encoding.current_vars.insert("result".to_string(), z3_val);
                    encoding.modified.insert("result".to_string());
                } else {
                    encoding.failures.push("return expression could not be encoded safely".to_string());
                }
            }
            _ => {}
        }
    }
}

// Collect all variable names that are assigned in a block
fn collect_modified_vars(block: &Block) -> HashSet<String> {
    let mut modified = HashSet::new();
    for stmt in &block.stmts {
        match stmt {
            Stmt::Assign {
                target: LValue::Ident(name),
                ..
            } => {
                modified.insert(name.clone());
            }
            Stmt::While { body, .. } => {
                modified.extend(collect_modified_vars(body));
            }
            Stmt::If {
                then_block,
                else_block,
                ..
            } => {
                modified.extend(collect_modified_vars(then_block));
                if let Some(eb) = else_block {
                    modified.extend(collect_modified_vars(eb));
                }
            }
            _ => {}
        }
    }
    modified
}

// Evaluate invariant expression using a single set of "current" variables
// Used for loop invariants where we don't have a pre/post split
fn invariant_to_z3_with_current<'ctx>(
    ctx: &'ctx Context,
    inv: &InvariantExpr,
    vars: &HashMap<String, BV<'ctx>>,
    signed_vars: &HashSet<String>,
) -> Option<Bool<'ctx>> {
    // Reuse the standard invariant_to_z3 by passing current vars as both pre and post
    invariant_to_z3(ctx, inv, vars, vars, signed_vars)
}

// Convert an Anvil Expr (used as a condition) to a Z3 Bool
// Handles comparisons like `counter > 0`, `i <= n`, and identifier-based conditions
fn condition_to_z3<'ctx>(
    ctx: &'ctx Context,
    expr: &Expr,
    vars: &HashMap<String, BV<'ctx>>,
    signed_vars: &HashSet<String>,
) -> Option<Bool<'ctx>> {
    match expr {
        Expr::BinOp { left, op, right } => match op {
            BinOp::Gt | BinOp::Lt | BinOp::Gte | BinOp::Lte | BinOp::Eq | BinOp::Neq => {
                let signed = expr_comparison_is_signed(left, right, signed_vars);
                if signed && (!signed_expr_literals_safe(left) || !signed_expr_literals_safe(right))
                {
                    return None;
                }
                let l = expr_to_z3(ctx, left, vars, signed_vars)?;
                let r = expr_to_z3(ctx, right, vars, signed_vars)?;
                Some(compare_expr_bv(&l, op, &r, signed))
            }
            BinOp::And => {
                let l = condition_to_z3(ctx, left, vars, signed_vars)?;
                let r = condition_to_z3(ctx, right, vars, signed_vars)?;
                Some(Bool::and(ctx, &[&l, &r]))
            }
            BinOp::Or => {
                let l = condition_to_z3(ctx, left, vars, signed_vars)?;
                let r = condition_to_z3(ctx, right, vars, signed_vars)?;
                Some(Bool::or(ctx, &[&l, &r]))
            }
            _ => None,
        },
        Expr::BoolLit(b) => Some(Bool::from_bool(ctx, *b)),
        Expr::Ident(name) => {
            // Boolean identifier: treat as `name != 0`
            if let Some(var) = vars.get(name.as_str()) {
                let zero = BV::from_i64(ctx, 0, SOLVER_BV_WIDTH);
                Some(var._eq(&zero).not())
            } else {
                None
            }
        }
        Expr::UnaryOp {
            op: UnaryOp::Not,
            operand,
        } => {
            let inner = condition_to_z3(ctx, operand, vars, signed_vars)?;
            Some(inner.not())
        }
        _ => None,
    }
}

// --- Type constraint injection ---
// Bridges the thermodynamic gap: Z3 Int is unbounded, but silicon is not.
// u64 has exactly 2^64 states. Every bit beyond that is a lie.

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

const I128_MIN_ABS_DEC: &str = "170141183460469231731687303715884105728";

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

fn bv_from_i128<'ctx>(ctx: &'ctx Context, value: i128) -> Option<BV<'ctx>> {
    if value >= 0 {
        BV::from_str(ctx, SOLVER_BV_WIDTH, &value.to_string())
    } else {
        let magnitude = BV::from_str(ctx, SOLVER_BV_WIDTH, &value.unsigned_abs().to_string())?;
        Some(BV::from_i64(ctx, 0, SOLVER_BV_WIDTH).bvsub(&magnitude))
    }
}

fn bv_from_decimal<'ctx>(ctx: &'ctx Context, value: &str) -> Option<BV<'ctx>> {
    let trimmed = value.trim();
    let magnitude = trimmed.strip_prefix('-').unwrap_or(trimmed);
    let bv = BV::from_str(ctx, SOLVER_BV_WIDTH, magnitude)?;
    if trimmed.starts_with('-') {
        Some(BV::from_i64(ctx, 0, SOLVER_BV_WIDTH).bvsub(&bv))
    } else {
        Some(bv)
    }
}

fn bv_pow2<'ctx>(ctx: &'ctx Context, bits: u32) -> Option<BV<'ctx>> {
    BV::from_str(ctx, SOLVER_BV_WIDTH, &decimal_pow2(bits))
}

fn inject_type_constraints<'ctx>(
    ctx: &'ctx Context,
    solver: &Solver<'ctx>,
    pre_vars: &HashMap<String, BV<'ctx>>,
    post_vars: &HashMap<String, BV<'ctx>>,
    constraints: &[TypeConstraint],
) {
    let zero = BV::from_i64(ctx, 0, SOLVER_BV_WIDTH);

    for constraint in constraints {
        // Apply to pre-state variable
        if let Some(pre_var) = pre_vars.get(&constraint.var_name) {
            inject_constraint_for_var(ctx, solver, pre_var, constraint, &zero);
        }

        // Apply to post-state variable (the result must also be in bounds)
        if let Some(post_var) = post_vars.get(&constraint.var_name) {
            inject_constraint_for_var(ctx, solver, post_var, constraint, &zero);
        }
    }
}

fn inject_constraints_for_var<'ctx>(
    ctx: &'ctx Context,
    solver: &Solver<'ctx>,
    var: &BV<'ctx>,
    constraints: &[TypeConstraint],
) {
    let zero = BV::from_i64(ctx, 0, SOLVER_BV_WIDTH);
    for constraint in constraints {
        inject_constraint_for_var(ctx, solver, var, constraint, &zero);
    }
}

fn inject_constraint_for_var<'ctx>(
    ctx: &'ctx Context,
    solver: &Solver<'ctx>,
    var: &BV<'ctx>,
    constraint: &TypeConstraint,
    zero: &BV<'ctx>,
) {
    match &constraint.kind {
        ConstraintKind::NonNegative => {
            solver.assert(&var.bvuge(zero));
        }
        ConstraintKind::UpperBound { bits } => {
            if *bits < SOLVER_BV_WIDTH {
                let upper = bv_pow2(ctx, *bits).unwrap();
                solver.assert(&var.bvult(&upper));
            }
        }
        ConstraintKind::SignedBound { bits } => {
            if *bits < SOLVER_BV_WIDTH {
                let upper = bv_pow2(ctx, *bits - 1).unwrap();
                let lower = BV::from_i64(ctx, 0, SOLVER_BV_WIDTH).bvsub(&upper);
                solver.assert(&var.bvsge(&lower));
                solver.assert(&var.bvslt(&upper));
            }
        }
    }
}

// --- Z3 translation ---

fn expr_to_z3<'ctx>(
    ctx: &'ctx Context,
    expr: &Expr,
    vars: &HashMap<String, BV<'ctx>>,
    signed_vars: &HashSet<String>,
) -> Option<BV<'ctx>> {
    match expr {
        Expr::IntLit(n) => bv_from_i128(ctx, *n),
        Expr::BigIntLit(n) => bv_from_decimal(ctx, n),
        Expr::BoolLit(b) => {
            // Encode booleans as integers: true=1, false=0
            Some(BV::from_i64(ctx, if *b { 1 } else { 0 }, SOLVER_BV_WIDTH))
        }
        Expr::Ident(name) => vars.get(name.trim()).cloned(),
        Expr::BinOp { left, op, right } => {
            match op {
                // Arithmetic operations
                BinOp::Add
                | BinOp::Sub
                | BinOp::Mul
                | BinOp::Div
                | BinOp::Mod
                | BinOp::BitAnd
                | BinOp::BitOr
                | BinOp::BitXor
                | BinOp::Shl
                | BinOp::Shr => {
                    let signed = expr_comparison_is_signed(left, right, signed_vars);
                    if signed
                        && (!signed_expr_literals_safe(left) || !signed_expr_literals_safe(right))
                    {
                        return None;
                    }
                    let l = expr_to_z3(ctx, left, vars, signed_vars)?;
                    let r = expr_to_z3(ctx, right, vars, signed_vars)?;
                    Some(match op {
                        BinOp::Add => l.bvadd(&r),
                        BinOp::Sub => l.bvsub(&r),
                        BinOp::Mul => l.bvmul(&r),
                        BinOp::Div if signed => l.bvsdiv(&r),
                        BinOp::Div => l.bvudiv(&r),
                        BinOp::Mod if signed => l.bvsrem(&r),
                        BinOp::Mod => l.bvurem(&r),
                        BinOp::BitAnd => l.bvand(&r),
                        BinOp::BitOr => l.bvor(&r),
                        BinOp::BitXor => l.bvxor(&r),
                        BinOp::Shl => l.bvshl(&r),
                        BinOp::Shr => l.bvlshr(&r),
                        _ => unreachable!(),
                    })
                }
                // Comparison/logical operations → encode as 0/1 integer
                BinOp::Eq
                | BinOp::Neq
                | BinOp::Lt
                | BinOp::Gt
                | BinOp::Lte
                | BinOp::Gte
                | BinOp::And
                | BinOp::Or => {
                    // These produce boolean results; encode as if-then-else: cond ? 1 : 0
                    let one = BV::from_i64(ctx, 1, SOLVER_BV_WIDTH);
                    let z = BV::from_i64(ctx, 0, SOLVER_BV_WIDTH);
                    match op {
                        BinOp::Eq
                        | BinOp::Neq
                        | BinOp::Lt
                        | BinOp::Gt
                        | BinOp::Lte
                        | BinOp::Gte => {
                            let signed = expr_comparison_is_signed(left, right, signed_vars);
                            if signed
                                && (!signed_expr_literals_safe(left)
                                    || !signed_expr_literals_safe(right))
                            {
                                return None;
                            }
                            let l = expr_to_z3(ctx, left, vars, signed_vars)?;
                            let r = expr_to_z3(ctx, right, vars, signed_vars)?;
                            let cond = compare_expr_bv(&l, op, &r, signed);
                            Some(cond.ite(&one, &z))
                        }
                        BinOp::And => {
                            let l = expr_to_z3(ctx, left, vars, signed_vars)?;
                            let r = expr_to_z3(ctx, right, vars, signed_vars)?;
                            let l_bool = l._eq(&z).not();
                            let r_bool = r._eq(&z).not();
                            let both = Bool::and(ctx, &[&l_bool, &r_bool]);
                            Some(both.ite(&one, &z))
                        }
                        BinOp::Or => {
                            let l = expr_to_z3(ctx, left, vars, signed_vars)?;
                            let r = expr_to_z3(ctx, right, vars, signed_vars)?;
                            let l_bool = l._eq(&z).not();
                            let r_bool = r._eq(&z).not();
                            let either = Bool::or(ctx, &[&l_bool, &r_bool]);
                            Some(either.ite(&one, &z))
                        }
                        _ => None,
                    }
                }
            }
        }
        Expr::UnaryOp { op, operand } => {
            let val = expr_to_z3(ctx, operand, vars, signed_vars)?;
            match op {
                UnaryOp::Neg => Some(BV::from_i64(ctx, 0, SOLVER_BV_WIDTH).bvsub(&val)),
                UnaryOp::Not => {
                    let z = BV::from_i64(ctx, 0, SOLVER_BV_WIDTH);
                    let one = BV::from_i64(ctx, 1, SOLVER_BV_WIDTH);
                    let is_zero = val._eq(&z);
                    Some(is_zero.ite(&one, &z))
                }
            }
        }
        Expr::FnCall { name, args } => {
            let z3_args: Option<Vec<BV<'ctx>>> = args
                .iter()
                .map(|arg| expr_to_z3(ctx, arg, vars, signed_vars))
                .collect();
            apply_uninterpreted_bv_fn(ctx, name, &z3_args?)
        }
        Expr::FieldAccess { object, field } => {
            // Field access: encode as {object}_{field} variable lookup
            let obj_name = match object.as_ref() {
                Expr::Ident(n) => n.clone(),
                _ => return None,
            };
            let composite = format!("{}_{}", obj_name, field);
            vars.get(&composite).cloned()
        }
        _ => None,
    }
}

fn apply_uninterpreted_bv_fn<'ctx>(
    ctx: &'ctx Context,
    name: &str,
    args: &[BV<'ctx>],
) -> Option<BV<'ctx>> {
    let bv_sort = Sort::bitvector(ctx, SOLVER_BV_WIDTH);
    let domain_sorts: Vec<Sort<'ctx>> = (0..args.len())
        .map(|_| Sort::bitvector(ctx, SOLVER_BV_WIDTH))
        .collect();
    let domain_refs: Vec<&Sort<'ctx>> = domain_sorts.iter().collect();
    let decl = FuncDecl::new(
        ctx,
        format!("__fn_{}_arity_{}", sanitize_symbol(name), args.len()),
        &domain_refs,
        &bv_sort,
    );
    let arg_refs: Vec<&dyn Ast<'ctx>> = args.iter().map(|arg| arg as &dyn Ast<'ctx>).collect();
    decl.apply(&arg_refs).try_into().ok()
}

fn sanitize_symbol(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn signed_expr_literals_safe(expr: &Expr) -> bool {
    match expr {
        Expr::BigIntLit(n) => decimal_lte(n, I128_MIN_ABS_DEC),
        Expr::BinOp { left, right, .. } => {
            signed_expr_literals_safe(left) && signed_expr_literals_safe(right)
        }
        Expr::UnaryOp { operand, .. } => signed_expr_literals_safe(operand),
        Expr::FnCall { args, .. } => args.iter().all(signed_expr_literals_safe),
        Expr::MethodCall { object, args, .. } => {
            signed_expr_literals_safe(object) && args.iter().all(signed_expr_literals_safe)
        }
        Expr::FieldAccess { object, .. } => signed_expr_literals_safe(object),
        Expr::Index { object, index } => {
            signed_expr_literals_safe(object) && signed_expr_literals_safe(index)
        }
        Expr::If {
            condition,
            then_block,
            else_block,
        } => {
            signed_expr_literals_safe(condition)
                && block_signed_literals_safe(then_block)
                && else_block
                    .as_ref()
                    .is_none_or(block_signed_literals_safe)
        }
        Expr::Block(block) => block_signed_literals_safe(block),
        _ => true,
    }
}

fn block_signed_literals_safe(block: &Block) -> bool {
    block.stmts.iter().all(|stmt| match stmt {
        Stmt::Let { value, .. } | Stmt::Assign { value, .. } | Stmt::Ghost { value, .. } => {
            signed_expr_literals_safe(value)
        }
        Stmt::Return(Some(expr))
        | Stmt::Assert {
            condition: expr, ..
        } => signed_expr_literals_safe(expr),
        Stmt::If {
            condition,
            then_block,
            else_block,
        } => {
            signed_expr_literals_safe(condition)
                && block_signed_literals_safe(then_block)
                && else_block
                    .as_ref()
                    .is_none_or(block_signed_literals_safe)
        }
        Stmt::While {
            condition, body, ..
        } => signed_expr_literals_safe(condition) && block_signed_literals_safe(body),
        Stmt::Emit { args, .. } => args.iter().all(signed_expr_literals_safe),
        Stmt::Expr(expr) => signed_expr_literals_safe(expr),
        _ => true,
    })
}

fn signed_inv_literals_safe(term: &InvTerm) -> bool {
    match term {
        InvTerm::BigLiteral(n) => decimal_lte(n, I128_MIN_ABS_DEC),
        InvTerm::BinOp { left, right, .. } => {
            signed_inv_literals_safe(left) && signed_inv_literals_safe(right)
        }
        InvTerm::FnCall { args, .. } => args.iter().all(signed_inv_literals_safe),
        _ => true,
    }
}

fn compare_expr_bv<'ctx>(
    left: &BV<'ctx>,
    op: &BinOp,
    right: &BV<'ctx>,
    signed: bool,
) -> Bool<'ctx> {
    match op {
        BinOp::Eq => left._eq(right),
        BinOp::Neq => left._eq(right).not(),
        BinOp::Lt if signed => left.bvslt(right),
        BinOp::Gt if signed => left.bvsgt(right),
        BinOp::Lte if signed => left.bvsle(right),
        BinOp::Gte if signed => left.bvsge(right),
        BinOp::Lt => left.bvult(right),
        BinOp::Gt => left.bvugt(right),
        BinOp::Lte => left.bvule(right),
        BinOp::Gte => left.bvuge(right),
        _ => unreachable!("non-comparison operator passed to compare_expr_bv"),
    }
}

fn compare_inv_bv<'ctx>(left: &BV<'ctx>, op: &CmpOp, right: &BV<'ctx>, signed: bool) -> Bool<'ctx> {
    match op {
        CmpOp::Eq => left._eq(right),
        CmpOp::Neq => left._eq(right).not(),
        CmpOp::Lt if signed => left.bvslt(right),
        CmpOp::Gt if signed => left.bvsgt(right),
        CmpOp::Lte if signed => left.bvsle(right),
        CmpOp::Gte if signed => left.bvsge(right),
        CmpOp::Lt => left.bvult(right),
        CmpOp::Gt => left.bvugt(right),
        CmpOp::Lte => left.bvule(right),
        CmpOp::Gte => left.bvuge(right),
    }
}

fn expr_comparison_is_signed(left: &Expr, right: &Expr, signed_vars: &HashSet<String>) -> bool {
    expr_uses_signed_var(left, signed_vars) || expr_uses_signed_var(right, signed_vars)
}

fn expr_uses_signed_var(expr: &Expr, signed_vars: &HashSet<String>) -> bool {
    match expr {
        Expr::Ident(name) => signed_vars.contains(name),
        Expr::BinOp { left, right, .. } => {
            expr_uses_signed_var(left, signed_vars) || expr_uses_signed_var(right, signed_vars)
        }
        Expr::UnaryOp { operand, .. } => expr_uses_signed_var(operand, signed_vars),
        Expr::FnCall { args, .. } => args
            .iter()
            .any(|arg| expr_uses_signed_var(arg, signed_vars)),
        Expr::MethodCall { object, args, .. } => {
            expr_uses_signed_var(object, signed_vars)
                || args
                    .iter()
                    .any(|arg| expr_uses_signed_var(arg, signed_vars))
        }
        Expr::FieldAccess { object, field } => {
            if let Expr::Ident(object_name) = object.as_ref() {
                signed_vars.contains(&format!("{}_{}", object_name, field))
            } else {
                expr_uses_signed_var(object, signed_vars)
            }
        }
        Expr::Index { object, index } => {
            expr_uses_signed_var(object, signed_vars) || expr_uses_signed_var(index, signed_vars)
        }
        Expr::If {
            condition,
            then_block,
            else_block,
        } => {
            expr_uses_signed_var(condition, signed_vars)
                || block_uses_signed_var(then_block, signed_vars)
                || else_block
                    .as_ref()
                    .is_some_and(|block| block_uses_signed_var(block, signed_vars))
        }
        Expr::Block(block) => block_uses_signed_var(block, signed_vars),
        _ => false,
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
                || else_block
                    .as_ref()
                    .is_some_and(block_contains_fncall)
        }
        Expr::Block(block) => block_contains_fncall(block),
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
                || else_block
                    .as_ref()
                    .is_some_and(block_contains_fncall)
        }
        Stmt::While {
            condition, body, ..
        } => expr_contains_fncall(condition) || block_contains_fncall(body),
        Stmt::Emit { args, .. } => args.iter().any(expr_contains_fncall),
        Stmt::Expr(expr) => expr_contains_fncall(expr),
        _ => false,
    })
}

fn block_uses_signed_var(block: &Block, signed_vars: &HashSet<String>) -> bool {
    block.stmts.iter().any(|stmt| match stmt {
        Stmt::Let { value, .. } | Stmt::Assign { value, .. } | Stmt::Ghost { value, .. } => {
            expr_uses_signed_var(value, signed_vars)
        }
        Stmt::Return(Some(expr))
        | Stmt::Assert {
            condition: expr, ..
        } => expr_uses_signed_var(expr, signed_vars),
        Stmt::If {
            condition,
            then_block,
            else_block,
        } => {
            expr_uses_signed_var(condition, signed_vars)
                || block_uses_signed_var(then_block, signed_vars)
                || else_block
                    .as_ref()
                    .is_some_and(|block| block_uses_signed_var(block, signed_vars))
        }
        Stmt::While {
            condition, body, ..
        } => {
            expr_uses_signed_var(condition, signed_vars) || block_uses_signed_var(body, signed_vars)
        }
        Stmt::Emit { args, .. } => args
            .iter()
            .any(|arg| expr_uses_signed_var(arg, signed_vars)),
        _ => false,
    })
}

fn inv_comparison_is_signed(
    left: &InvTerm,
    right: &InvTerm,
    signed_vars: &HashSet<String>,
) -> bool {
    inv_term_uses_signed_var(left, signed_vars) || inv_term_uses_signed_var(right, signed_vars)
}

fn inv_term_uses_signed_var(term: &InvTerm, signed_vars: &HashSet<String>) -> bool {
    match term {
        InvTerm::Var { name, .. } => signed_vars.contains(name),
        InvTerm::FieldAccess { object, field, .. } => {
            signed_vars.contains(&format!("{}_{}", object, field))
        }
        InvTerm::BinOp { left, right, .. } => {
            inv_term_uses_signed_var(left, signed_vars)
                || inv_term_uses_signed_var(right, signed_vars)
        }
        InvTerm::FnCall { args, .. } => args
            .iter()
            .any(|arg| inv_term_uses_signed_var(arg, signed_vars)),
        InvTerm::Literal(_) | InvTerm::BigLiteral(_) => false,
    }
}

fn invariant_to_z3<'ctx>(
    ctx: &'ctx Context,
    inv: &InvariantExpr,
    pre_vars: &HashMap<String, BV<'ctx>>,
    post_vars: &HashMap<String, BV<'ctx>>,
    signed_vars: &HashSet<String>,
) -> Option<Bool<'ctx>> {
    match inv {
        InvariantExpr::Comparison { left, op, right } => {
            let signed = inv_comparison_is_signed(left, right, signed_vars);
            if signed && (!signed_inv_literals_safe(left) || !signed_inv_literals_safe(right)) {
                return None;
            }
            let l = inv_term_to_z3(ctx, left, pre_vars, post_vars, signed_vars)?;
            let r = inv_term_to_z3(ctx, right, pre_vars, post_vars, signed_vars)?;
            Some(compare_inv_bv(&l, op, &r, signed))
        }
        InvariantExpr::And(a, b) => {
            let (za, zb) = (
                invariant_to_z3(ctx, a, pre_vars, post_vars, signed_vars)?,
                invariant_to_z3(ctx, b, pre_vars, post_vars, signed_vars)?,
            );
            Some(Bool::and(ctx, &[&za, &zb]))
        }
        InvariantExpr::Or(a, b) => {
            let (za, zb) = (
                invariant_to_z3(ctx, a, pre_vars, post_vars, signed_vars)?,
                invariant_to_z3(ctx, b, pre_vars, post_vars, signed_vars)?,
            );
            Some(Bool::or(ctx, &[&za, &zb]))
        }
        InvariantExpr::Not(a) => {
            Some(invariant_to_z3(ctx, a, pre_vars, post_vars, signed_vars)?.not())
        }
        InvariantExpr::True => Some(Bool::from_bool(ctx, true)),
        InvariantExpr::Forall { var, domain, body } => {
            // Quantified assertion: ∀ var ∈ [0, domain) : body
            // Create a bounded integer variable for the quantifier
            let bound_var = BV::new_const(ctx, var.as_str(), SOLVER_BV_WIDTH);
            let zero = BV::from_i64(ctx, 0, SOLVER_BV_WIDTH);
            let domain_z3 = inv_term_to_z3(ctx, domain, pre_vars, post_vars, signed_vars)?;

            // Construct: ∀ bound_var: (0 <= bound_var < domain) => body
            // We encode this as: NOT EXISTS bound_var: (0 <= bound_var < domain) AND NOT body
            // Which Z3 can handle via its quantifier reasoning
            let mut augmented_pre = pre_vars.clone();
            augmented_pre.insert(var.clone(), bound_var.clone());
            let mut augmented_post = post_vars.clone();
            augmented_post.insert(var.clone(), bound_var.clone());

            let body_z3 = invariant_to_z3(ctx, body, &augmented_pre, &augmented_post, signed_vars)?;
            let range = Bool::and(
                ctx,
                &[&bound_var.bvuge(&zero), &bound_var.bvult(&domain_z3)],
            );
            let implication = range.implies(&body_z3);

            // Use Z3's native forall
            let pattern = z3::Pattern::new(ctx, &[&bound_var as &dyn Ast]);
            let quantified = z3::ast::forall_const(ctx, &[&bound_var], &[&pattern], &implication);
            Some(quantified)
        }
        InvariantExpr::Exists { var, domain, body } => {
            // Existential: ∃ var ∈ [0, domain) : body
            let bound_var = BV::new_const(ctx, var.as_str(), SOLVER_BV_WIDTH);
            let zero = BV::from_i64(ctx, 0, SOLVER_BV_WIDTH);
            let domain_z3 = inv_term_to_z3(ctx, domain, pre_vars, post_vars, signed_vars)?;

            let mut augmented_pre = pre_vars.clone();
            augmented_pre.insert(var.clone(), bound_var.clone());
            let mut augmented_post = post_vars.clone();
            augmented_post.insert(var.clone(), bound_var.clone());

            let body_z3 = invariant_to_z3(ctx, body, &augmented_pre, &augmented_post, signed_vars)?;
            let range = Bool::and(
                ctx,
                &[&bound_var.bvuge(&zero), &bound_var.bvult(&domain_z3)],
            );
            let conjunction = Bool::and(ctx, &[&range, &body_z3]);

            let pattern = z3::Pattern::new(ctx, &[&bound_var as &dyn Ast]);
            let quantified = z3::ast::exists_const(ctx, &[&bound_var], &[&pattern], &conjunction);
            Some(quantified)
        }
    }
}

fn inv_term_to_z3<'ctx>(
    ctx: &'ctx Context,
    term: &InvTerm,
    pre_vars: &HashMap<String, BV<'ctx>>,
    post_vars: &HashMap<String, BV<'ctx>>,
    signed_vars: &HashSet<String>,
) -> Option<BV<'ctx>> {
    match term {
        InvTerm::Literal(n) => bv_from_i128(ctx, *n),
        InvTerm::BigLiteral(n) => bv_from_decimal(ctx, n),
        InvTerm::Var { name, is_post } => {
            if *is_post {
                post_vars.get(name).cloned()
            } else {
                pre_vars.get(name).cloned()
            }
        }
        InvTerm::BinOp { left, op, right } => {
            let signed = inv_comparison_is_signed(left, right, signed_vars);
            if signed && (!signed_inv_literals_safe(left) || !signed_inv_literals_safe(right)) {
                return None;
            }
            let l = inv_term_to_z3(ctx, left, pre_vars, post_vars, signed_vars)?;
            let r = inv_term_to_z3(ctx, right, pre_vars, post_vars, signed_vars)?;
            Some(match op {
                ArithOp::Add => l.bvadd(&r),
                ArithOp::Sub => l.bvsub(&r),
                ArithOp::Mul => l.bvmul(&r),
                ArithOp::Div if signed => l.bvsdiv(&r),
                ArithOp::Div => l.bvudiv(&r),
                ArithOp::Mod if signed => l.bvsrem(&r),
                ArithOp::Mod => l.bvurem(&r),
                ArithOp::BitAnd => l.bvand(&r),
                ArithOp::BitOr => l.bvor(&r),
                ArithOp::BitXor => l.bvxor(&r),
                ArithOp::Shl => l.bvshl(&r),
                ArithOp::Shr => l.bvlshr(&r),
            })
        }
        InvTerm::FieldAccess {
            object,
            field,
            is_post,
        } => {
            let n = format!("{}_{}", object, field);
            if *is_post {
                post_vars.get(&n).cloned()
            } else {
                pre_vars.get(&n).cloned()
            }
        }
        InvTerm::FnCall { name, args } => {
            let z3_args: Option<Vec<BV<'ctx>>> = args
                .iter()
                .map(|arg| inv_term_to_z3(ctx, arg, pre_vars, post_vars, signed_vars))
                .collect();
            apply_uninterpreted_bv_fn(ctx, name, &z3_args?)
        }
    }
}

// --- Pretty printing ---

pub fn print_results(results: &[VerifyResult]) {
    println!();
    println!(
        "{}",
        "╔══════════════════════════════════════════════════╗".bright_blue()
    );
    println!(
        "{}",
        "║         ANVIL VERIFICATION REPORT                ║".bright_blue()
    );
    println!(
        "{}",
        "╚══════════════════════════════════════════════════╝".bright_blue()
    );
    println!();

    let mut total = 0;
    let mut passed = 0;

    for r in results {
        total += r.postconditions_count;
        if r.verified {
            passed += r.postconditions_count;
            println!(
                "  {} {} — {} preconditions, {} postconditions verified ({} invariants) in {:.3}ms",
                "✓".bright_green().bold(),
                r.fn_name.bright_white().bold(),
                r.preconditions_count,
                r.postconditions_count,
                r.invariants_checked,
                r.duration_ms,
            );
            println!(
                "    🔐 proof: {}…{}",
                &r.proof_hash[..16],
                &r.proof_hash[r.proof_hash.len() - 8..],
            );
        } else {
            println!(
                "  {} {} — VERIFICATION FAILED",
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
        println!(
            "  {} All {}/{} postconditions proven. Zero trust required.",
            "█".bright_green(),
            passed,
            total
        );
    } else if total == 0 {
        println!("  {} No postconditions to verify.", "█".bright_yellow());
    } else {
        println!(
            "  {} {}/{} postconditions proven. {} FAILED.",
            "█".bright_red(),
            passed,
            total,
            total - passed
        );
    }
    println!();
}

// ============================================================
// ALGEBRAIC SIMPLIFICATION & CONSTANT PROPAGATION ENGINE
// ============================================================

fn collect_known_constants(preconditions: &[&Invariant]) -> HashMap<String, i128> {
    let mut constants = HashMap::new();
    for pre in preconditions {
        if let InvariantExpr::Comparison { left, op: CmpOp::Eq, right } = &pre.expr {
            match (left.as_ref(), right.as_ref()) {
                (InvTerm::Var { name, is_post: false }, right_term) => {
                    if let Some(val) = get_literal_val(right_term) {
                        constants.insert(name.clone(), val);
                    }
                }
                (left_term, InvTerm::Var { name, is_post: false }) => {
                    if let Some(val) = get_literal_val(left_term) {
                        constants.insert(name.clone(), val);
                    }
                }
                _ => {}
            }
        }
    }
    constants
}

fn get_literal_val(term: &InvTerm) -> Option<i128> {
    match term {
        InvTerm::Literal(val) => Some(*val),
        InvTerm::BigLiteral(s) => s.parse::<i128>().ok(),
        _ => None,
    }
}

fn get_expr_literal_val(expr: &Expr) -> Option<i128> {
    match expr {
        Expr::IntLit(val) => Some(*val),
        Expr::BigIntLit(s) => s.parse::<i128>().ok(),
        _ => None,
    }
}

fn gcd(mut a: i128, mut b: i128) -> i128 {
    while b != 0 {
        let temp = b;
        b = a % b;
        a = temp;
    }
    a.abs()
}

fn is_power_of_two(n: i128) -> Option<u32> {
    if n <= 0 {
        return None;
    }
    if (n & (n - 1)) == 0 {
        Some(n.trailing_zeros())
    } else {
        None
    }
}

fn simplify_inv_term(term: &InvTerm, constants: &HashMap<String, i128>, in_arith_op: bool) -> InvTerm {
    match term {
        InvTerm::Var { name, is_post } => {
            if !*is_post && in_arith_op {
                if let Some(&val) = constants.get(name) {
                    return InvTerm::Literal(val);
                }
            }
            term.clone()
        }
        InvTerm::BinOp { left, op, right } => {
            let left_sim = simplify_inv_term(left, constants, true);
            let right_sim = simplify_inv_term(right, constants, true);

            // Algebraic simplification for (X * a) / b
            if *op == ArithOp::Div {
                if let Some(b) = get_literal_val(&right_sim) {
                    if b != 0 {
                        // Check if left is (X * a) or (a * X)
                        if let InvTerm::BinOp { left: ref xl, op: ArithOp::Mul, right: ref xr } = left_sim {
                            let (x, r_term) = (xl.as_ref(), xr.as_ref());
                            if let Some(a) = get_literal_val(r_term) {
                                let g = gcd(a, b);
                                let a_prime = a / g;
                                let b_prime = b / g;
                                if a_prime == 1 {
                                    if let Some(k) = is_power_of_two(b_prime) {
                                        return InvTerm::BinOp {
                                            left: Box::new(x.clone()),
                                            op: ArithOp::Shr,
                                            right: Box::new(InvTerm::Literal(k as i128)),
                                        };
                                    } else {
                                        return InvTerm::BinOp {
                                            left: Box::new(x.clone()),
                                            op: ArithOp::Div,
                                            right: Box::new(InvTerm::Literal(b_prime)),
                                        };
                                    }
                                } else {
                                    let new_left = InvTerm::BinOp {
                                        left: Box::new(x.clone()),
                                        op: ArithOp::Mul,
                                        right: Box::new(InvTerm::Literal(a_prime)),
                                    };
                                    return InvTerm::BinOp {
                                        left: Box::new(new_left),
                                        op: ArithOp::Div,
                                        right: Box::new(InvTerm::Literal(b_prime)),
                                    };
                                }
                            }
                        }
                        if let InvTerm::BinOp { left: ref xl, op: ArithOp::Mul, right: ref xr } = left_sim {
                            let (l_term, x) = (xl.as_ref(), xr.as_ref());
                            if let Some(a) = get_literal_val(l_term) {
                                let g = gcd(a, b);
                                let a_prime = a / g;
                                let b_prime = b / g;
                                if a_prime == 1 {
                                    if let Some(k) = is_power_of_two(b_prime) {
                                        return InvTerm::BinOp {
                                            left: Box::new(x.clone()),
                                            op: ArithOp::Shr,
                                            right: Box::new(InvTerm::Literal(k as i128)),
                                        };
                                    } else {
                                        return InvTerm::BinOp {
                                            left: Box::new(x.clone()),
                                            op: ArithOp::Div,
                                            right: Box::new(InvTerm::Literal(b_prime)),
                                        };
                                    }
                                } else {
                                    let new_left = InvTerm::BinOp {
                                        left: Box::new(InvTerm::Literal(a_prime)),
                                        op: ArithOp::Mul,
                                        right: Box::new(x.clone()),
                                    };
                                    return InvTerm::BinOp {
                                        left: Box::new(new_left),
                                        op: ArithOp::Div,
                                        right: Box::new(InvTerm::Literal(b_prime)),
                                    };
                                }
                            }
                        }

                        // Divisor is a power of two: X / b -> X >> k
                        if let Some(k) = is_power_of_two(b) {
                            return InvTerm::BinOp {
                                left: Box::new(left_sim),
                                op: ArithOp::Shr,
                                  right: Box::new(InvTerm::Literal(k as i128)),
                            };
                        }
                    }
                }
            }

            // Constant folding
            if let (Some(a), Some(b)) = (get_literal_val(&left_sim), get_literal_val(&right_sim)) {
                let folded = match op {
                    ArithOp::Add => Some(a.wrapping_add(b)),
                    ArithOp::Sub => Some(a.wrapping_sub(b)),
                    ArithOp::Mul => Some(a.wrapping_mul(b)),
                    ArithOp::Div => if b != 0 { Some(a.wrapping_div(b)) } else { None },
                    ArithOp::Mod => if b != 0 { Some(a.wrapping_rem(b)) } else { None },
                    ArithOp::BitAnd => Some(a & b),
                    ArithOp::BitOr => Some(a | b),
                    ArithOp::BitXor => Some(a ^ b),
                    ArithOp::Shl => Some(a.wrapping_shl(b as u32)),
                    ArithOp::Shr => Some(a.wrapping_shr(b as u32)),
                };
                if let Some(val) = folded {
                    return InvTerm::Literal(val);
                }
            }

            InvTerm::BinOp {
                left: Box::new(left_sim),
                op: op.clone(),
                right: Box::new(right_sim),
            }
        }
        InvTerm::FnCall { name, args } => {
            let args_sim = args.iter().map(|arg| simplify_inv_term(arg, constants, false)).collect();
            InvTerm::FnCall { name: name.clone(), args: args_sim }
        }
        _ => term.clone(),
    }
}

fn simplify_invariant_expr(expr: &InvariantExpr, constants: &HashMap<String, i128>) -> InvariantExpr {
    match expr {
        InvariantExpr::Comparison { left, op, right } => InvariantExpr::Comparison {
            left: Box::new(simplify_inv_term(left, constants, false)),
            op: op.clone(),
            right: Box::new(simplify_inv_term(right, constants, false)),
        },
        InvariantExpr::And(a, b) => InvariantExpr::And(
            Box::new(simplify_invariant_expr(a, constants)),
            Box::new(simplify_invariant_expr(b, constants)),
        ),
        InvariantExpr::Or(a, b) => InvariantExpr::Or(
            Box::new(simplify_invariant_expr(a, constants)),
            Box::new(simplify_invariant_expr(b, constants)),
        ),
        InvariantExpr::Not(a) => InvariantExpr::Not(Box::new(simplify_invariant_expr(a, constants))),
        InvariantExpr::Forall { var, domain, body } => InvariantExpr::Forall {
            var: var.clone(),
            domain: Box::new(simplify_inv_term(domain, constants, false)),
            body: Box::new(simplify_invariant_expr(body, constants)),
        },
        InvariantExpr::Exists { var, domain, body } => InvariantExpr::Exists {
            var: var.clone(),
            domain: Box::new(simplify_inv_term(domain, constants, false)),
            body: Box::new(simplify_invariant_expr(body, constants)),
        },
        InvariantExpr::True => InvariantExpr::True,
    }
}

fn simplify_expr(expr: &Expr, constants: &HashMap<String, i128>, in_arith_op: bool) -> Expr {
    match expr {
        Expr::Ident(name) => {
            if in_arith_op {
                if let Some(&val) = constants.get(name) {
                    return Expr::IntLit(val);
                }
            }
            expr.clone()
        }
        Expr::BinOp { left, op, right } => {
            let is_arith = matches!(
                op,
                BinOp::Add
                    | BinOp::Sub
                    | BinOp::Mul
                    | BinOp::Div
                    | BinOp::Mod
                    | BinOp::BitAnd
                    | BinOp::BitOr
                    | BinOp::BitXor
                    | BinOp::Shl
                    | BinOp::Shr
            );
            let left_sim = simplify_expr(left, constants, is_arith);
            let right_sim = simplify_expr(right, constants, is_arith);

            // Algebraic simplification for (X * a) / b
            if *op == BinOp::Div {
                if let Some(b) = get_expr_literal_val(&right_sim) {
                    if b != 0 {
                        if let Expr::BinOp { left: ref xl, op: BinOp::Mul, right: ref xr } = left_sim {
                            let (x, r_term) = (xl.as_ref(), xr.as_ref());
                            if let Some(a) = get_expr_literal_val(r_term) {
                                let g = gcd(a, b);
                                let a_prime = a / g;
                                let b_prime = b / g;
                                if a_prime == 1 {
                                    if let Some(k) = is_power_of_two(b_prime) {
                                        return Expr::BinOp {
                                            left: Box::new(x.clone()),
                                            op: BinOp::Shr,
                                            right: Box::new(Expr::IntLit(k as i128)),
                                        };
                                    } else {
                                        return Expr::BinOp {
                                            left: Box::new(x.clone()),
                                            op: BinOp::Div,
                                            right: Box::new(Expr::IntLit(b_prime)),
                                        };
                                    }
                                } else {
                                    let new_left = Expr::BinOp {
                                        left: Box::new(x.clone()),
                                        op: BinOp::Mul,
                                        right: Box::new(Expr::IntLit(a_prime)),
                                    };
                                    return Expr::BinOp {
                                        left: Box::new(new_left),
                                        op: BinOp::Div,
                                        right: Box::new(Expr::IntLit(b_prime)),
                                    };
                                }
                            }
                        }
                        if let Expr::BinOp { left: ref xl, op: BinOp::Mul, right: ref xr } = left_sim {
                            let (l_term, x) = (xl.as_ref(), xr.as_ref());
                            if let Some(a) = get_expr_literal_val(l_term) {
                                let g = gcd(a, b);
                                let a_prime = a / g;
                                let b_prime = b / g;
                                if a_prime == 1 {
                                    if let Some(k) = is_power_of_two(b_prime) {
                                        return Expr::BinOp {
                                            left: Box::new(x.clone()),
                                            op: BinOp::Shr,
                                            right: Box::new(Expr::IntLit(k as i128)),
                                        };
                                    } else {
                                        return Expr::BinOp {
                                            left: Box::new(x.clone()),
                                            op: BinOp::Div,
                                            right: Box::new(Expr::IntLit(b_prime)),
                                        };
                                    }
                                } else {
                                    let new_left = Expr::BinOp {
                                        left: Box::new(Expr::IntLit(a_prime)),
                                        op: BinOp::Mul,
                                        right: Box::new(x.clone()),
                                    };
                                    return Expr::BinOp {
                                        left: Box::new(new_left),
                                        op: BinOp::Div,
                                        right: Box::new(Expr::IntLit(b_prime)),
                                    };
                                }
                            }
                        }

                        if let Some(k) = is_power_of_two(b) {
                            return Expr::BinOp {
                                left: Box::new(left_sim),
                                op: BinOp::Shr,
                                right: Box::new(Expr::IntLit(k as i128)),
                            };
                        }
                    }
                }
            }

            // Constant folding
            if let (Some(a), Some(b)) = (get_expr_literal_val(&left_sim), get_expr_literal_val(&right_sim)) {
                let folded = match op {
                    BinOp::Add => Some(a.wrapping_add(b)),
                    BinOp::Sub => Some(a.wrapping_sub(b)),
                    BinOp::Mul => Some(a.wrapping_mul(b)),
                    BinOp::Div => if b != 0 { Some(a.wrapping_div(b)) } else { None },
                    BinOp::Mod => if b != 0 { Some(a.wrapping_rem(b)) } else { None },
                    BinOp::BitAnd => Some(a & b),
                    BinOp::BitOr => Some(a | b),
                    BinOp::BitXor => Some(a ^ b),
                    BinOp::Shl => Some(a.wrapping_shl(b as u32)),
                    BinOp::Shr => Some(a.wrapping_shr(b as u32)),
                    _ => None,
                };
                if let Some(val) = folded {
                    return Expr::IntLit(val);
                }
            }

            Expr::BinOp {
                left: Box::new(left_sim),
                op: op.clone(),
                right: Box::new(right_sim),
            }
        }
        Expr::UnaryOp { op, operand } => {
            let is_arith = *op == UnaryOp::Neg;
            let operand_sim = simplify_expr(operand, constants, is_arith);
            if let Some(val) = get_expr_literal_val(&operand_sim) {
                let folded = match op {
                    UnaryOp::Neg => Some(val.wrapping_neg()),
                    UnaryOp::Not => Some(if val == 0 { 1 } else { 0 }),
                };
                if let Some(v) = folded {
                    return Expr::IntLit(v);
                }
            }
            Expr::UnaryOp { op: op.clone(), operand: Box::new(operand_sim) }
        }
        Expr::FnCall { name, args } => {
            let args_sim = args.iter().map(|arg| simplify_expr(arg, constants, false)).collect();
            Expr::FnCall { name: name.clone(), args: args_sim }
        }
        Expr::MethodCall { object, method, args } => Expr::MethodCall {
            object: Box::new(simplify_expr(object, constants, false)),
            method: method.clone(),
            args: args.iter().map(|e| simplify_expr(e, constants, false)).collect(),
        },
        Expr::FieldAccess { object, field } => Expr::FieldAccess {
            object: Box::new(simplify_expr(object, constants, false)),
            field: field.clone(),
        },
        Expr::Index { object, index } => Expr::Index {
            object: Box::new(simplify_expr(object, constants, false)),
            index: Box::new(simplify_expr(index, constants, false)),
        },
        Expr::If { condition, then_block, else_block } => {
            let cond_sim = simplify_expr(condition, constants, false);
            let then_sim = simplify_block(then_block, constants);
            let else_sim = else_block.as_ref().map(|b| simplify_block(b, constants));
            Expr::If {
                condition: Box::new(cond_sim),
                then_block: then_sim,
                else_block: else_sim,
            }
        }
        Expr::Block(block) => Expr::Block(simplify_block(block, constants)),
        _ => expr.clone(),
    }
}

fn simplify_block(block: &Block, constants: &HashMap<String, i128>) -> Block {
    let stmts_sim = block.stmts.iter().map(|stmt| simplify_stmt(stmt, constants)).collect();
    let expr_sim = block.expr.as_ref().map(|e| Box::new(simplify_expr(e, constants, false)));
    Block { stmts: stmts_sim, expr: expr_sim }
}

fn simplify_lvalue(lval: &LValue, constants: &HashMap<String, i128>) -> LValue {
    match lval {
        LValue::Ident(name) => LValue::Ident(name.clone()),
        LValue::FieldAccess { object, field } => LValue::FieldAccess {
            object: object.clone(),
            field: field.clone(),
        },
        LValue::Index { object, index } => LValue::Index {
            object: object.clone(),
            index: Box::new(simplify_expr(index, constants, false)),
        },
    }
}

fn simplify_stmt(stmt: &Stmt, constants: &HashMap<String, i128>) -> Stmt {
    match stmt {
        Stmt::Let { name, ty, is_mut, value } => Stmt::Let {
            name: name.clone(),
            ty: ty.clone(),
            is_mut: *is_mut,
            value: simplify_expr(value, constants, false),
        },
        Stmt::Assign { target, op, value } => Stmt::Assign {
            target: simplify_lvalue(target, constants),
            op: op.clone(),
            value: simplify_expr(value, constants, false),
        },
        Stmt::If { condition, then_block, else_block } => Stmt::If {
            condition: simplify_expr(condition, constants, false),
            then_block: simplify_block(then_block, constants),
            else_block: else_block.as_ref().map(|b| simplify_block(b, constants)),
        },
        Stmt::While { condition, invariants, body } => {
            let invs_sim = invariants.iter().map(|inv| Invariant {
                expr: simplify_invariant_expr(&inv.expr, constants),
                span: inv.span.clone(),
            }).collect();
            Stmt::While {
                condition: simplify_expr(condition, constants, false),
                invariants: invs_sim,
                body: simplify_block(body, constants),
            }
        }
        Stmt::Return(expr) => Stmt::Return(expr.as_ref().map(|e| simplify_expr(e, constants, false))),
        Stmt::Assert { condition, message } => Stmt::Assert {
            condition: simplify_expr(condition, constants, false),
            message: message.clone(),
        },
        Stmt::Emit { event, args } => Stmt::Emit {
            event: event.clone(),
            args: args.iter().map(|e| simplify_expr(e, constants, false)).collect(),
        },
        Stmt::Ghost { name, ty, value } => Stmt::Ghost {
            name: name.clone(),
            ty: ty.clone(),
            value: simplify_expr(value, constants, false),
        },
        Stmt::Expr(expr) => Stmt::Expr(simplify_expr(expr, constants, false)),
    }
}

