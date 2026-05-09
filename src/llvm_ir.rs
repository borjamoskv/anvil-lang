// ============================================================
// ANVIL LLVM IR GENERATOR (Direct-Silicon JIT)
// Bypasses the Rust compiler entirely.
// Emits raw LLVM Intermediate Representation (.ll)
// Only called AFTER Z3 guarantees the invariants.
// ============================================================

use crate::ast::*;

pub fn generate_llvm_ir(program: &Program) -> String {
    let mut out = String::new();
    out.push_str("; ==========================================\n");
    out.push_str("; ANVIL DIRECT-SILICON JIT EMITTER\n");
    out.push_str("; Math Proven by Z3. Zero Runtime Checks.\n");
    out.push_str("; ==========================================\n\n");
    out.push_str("source_filename = \"anvil_module\"\n\n");

    for item in &program.items {
        if let Item::Function(f) = item {
            out.push_str(&gen_llvm_function(f));
        }
    }

    out
}

fn gen_llvm_function(f: &FnDef) -> String {
    let mut out = String::new();
    let mut vreg_counter = 1; // Virtual registers %1, %2, ...

    // Signature
    out.push_str(&format!("define {} @{}(", gen_llvm_type(&f.return_type), f.name));
    
    let params: Vec<String> = f.params.iter().map(|p| {
        format!("{} %{}", gen_llvm_type(&Some(p.ty.clone())), p.name)
    }).collect();
    out.push_str(&params.join(", "));
    out.push_str(") {\nentry:\n");

    // Local Variables mapping (in a real emitter we'd use alloca, but for SSA we can just map)
    // For simplicity in this artifact, we will do basic block emission.
    for stmt in &f.body.stmts {
        match stmt {
            Stmt::Let { name, value, .. } => {
                let (val_str, next_vreg) = gen_llvm_expr(value, vreg_counter);
                vreg_counter = next_vreg;
                out.push_str(&format!("  %{} = {}\n", name, val_str));
            },
            Stmt::Assign { target, op: AssignOp::SubAssign, value } => {
                let target_name = match target {
                    LValue::Ident(n) => n.clone(),
                    _ => "unknown".to_string()
                };
                let (val_str, next_vreg) = gen_llvm_expr(value, vreg_counter);
                vreg_counter = next_vreg;
                out.push_str(&format!("  %{} = sub i64 %{}, {}\n", vreg_counter, target_name, val_str));
                vreg_counter += 1;
            },
            Stmt::Assign { target, op: AssignOp::AddAssign, value } => {
                let target_name = match target {
                    LValue::Ident(n) => n.clone(),
                    _ => "unknown".to_string()
                };
                let (val_str, next_vreg) = gen_llvm_expr(value, vreg_counter);
                vreg_counter = next_vreg;
                out.push_str(&format!("  %{} = add i64 %{}, {}\n", vreg_counter, target_name, val_str));
                vreg_counter += 1;
            },
            Stmt::Return(Some(expr)) => {
                let (val_str, next_vreg) = gen_llvm_expr(expr, vreg_counter);
                vreg_counter = next_vreg;
                out.push_str(&format!("  ret i64 {}\n", val_str));
            },
            _ => {
                out.push_str("  ; Unimplemented statement mapping\n");
            }
        }
    }

    // Fallback return if block ended
    if f.return_type.is_none() {
        out.push_str("  ret void\n");
    }

    out.push_str("}\n\n");
    out
}

fn gen_llvm_type(ty: &Option<Type>) -> String {
    match ty {
        Some(Type::U64) | Some(Type::I64) => "i64".into(),
        Some(Type::U32) | Some(Type::I32) => "i32".into(),
        Some(Type::U8) | Some(Type::I8) => "i8".into(),
        Some(Type::Bool) => "i1".into(),
        _ => "void".into(),
    }
}

fn gen_llvm_expr(expr: &Expr, vreg: usize) -> (String, usize) {
    match expr {
        Expr::IntLit(n) => (n.to_string(), vreg),
        Expr::Ident(name) => (format!("%{}", name), vreg),
        _ => ("0".to_string(), vreg) // Placeholder for complex exprs
    }
}
