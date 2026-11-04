use sovereign_c::lexer::Token;
use sovereign_c::parser::Parser;
use sovereign_c::borrowck::{BorrowContext, check_statement};
use sovereign_c::lowering::IrLoweringVisitor;
use sovereign_c::ir::IrOp;
use sovereign_c::backend::generate_x86_64_from_ir;
use logos::Logos;
use std::collections::HashSet;

#[test]
fn test_affine_alloc_and_synthetic_drop_lowering() {
    let code = r#"
    {
        let mem: owned u8 = 512;
    }
    "#;
    let lexer = Token::lexer(code);
    let tokens: Vec<Token> = lexer.filter_map(Result::ok).collect();
    let mut parser = Parser::new(tokens.into_iter());
    let ast = parser.parse_block().expect("Parse failed");

    let mut ctx = BorrowContext::new();
    let transformed = check_statement(&ast, &mut ctx).expect("Borrowck failed");

    let visitor = IrLoweringVisitor::new();
    let ir = visitor.lower_ast(&transformed, "test_alloc_drop");

    // Verificar que existe Alloc y Free sobre el mismo registro SSA
    let mut allocated_reg = None;
    let mut freed_reg = None;

    for block in &ir.blocks {
        for op in &block.instructions {
            match op {
                IrOp::Alloc { dest, size } => {
                    assert_eq!(*size, 512);
                    allocated_reg = Some(*dest);
                }
                IrOp::Free { ptr } => {
                    freed_reg = Some(*ptr);
                }
                _ => {}
            }
        }
    }

    assert!(allocated_reg.is_some(), "Debe existir instrucción Alloc");
    assert!(freed_reg.is_some(), "Debe existir instrucción Free para SyntheticDrop");
    assert_eq!(allocated_reg.unwrap(), freed_reg.unwrap(), "El registro liberado debe coincidir unívocamente con el asignado");
}

#[test]
fn test_mmio_volatile_store_barrier() {
    let code = r#"
    {
        unsafe {
            let port: raw u32 = 65536;
            *port = 255;
        }
    }
    "#;
    let lexer = Token::lexer(code);
    let tokens: Vec<Token> = lexer.filter_map(Result::ok).collect();
    let mut parser = Parser::new(tokens.into_iter());
    let ast = parser.parse_block().expect("Parse failed");

    let mut ctx = BorrowContext::new();
    let transformed = check_statement(&ast, &mut ctx).expect("Borrowck failed");

    let visitor = IrLoweringVisitor::new();
    let ir = visitor.lower_ast(&transformed, "test_volatile");

    let mut has_volatile_store = false;
    for block in &ir.blocks {
        for op in &block.instructions {
            if let IrOp::VolatileStore { ptr: _, value: _ } = op {
                has_volatile_store = true;
            }
        }
    }

    assert!(has_volatile_store, "Todo acceso a raw ptr en unsafe debe emitir VolatileStore");

    let asm = generate_x86_64_from_ir(&ir);
    assert!(asm.contains("VOLATILE STORE RING-0 MMIO"), "El ensamblador debe contener el marcador de barrera volátil");
    assert!(asm.contains("mfence"), "El ensamblador debe emitir la instrucción mfence");
}

#[test]
fn test_ssa_single_assignment_invariant() {
    let code = r#"
    {
        let buffer: owned u8 = 1024;
        let number: u32 = 42;
        
        unsafe {
            let mmio_port: raw u32 = 400000;
            *mmio_port = number;
        }
    }
    "#;
    let lexer = Token::lexer(code);
    let tokens: Vec<Token> = lexer.filter_map(Result::ok).collect();
    let mut parser = Parser::new(tokens.into_iter());
    let ast = parser.parse_block().expect("Parse failed");

    let mut ctx = BorrowContext::new();
    let transformed = check_statement(&ast, &mut ctx).expect("Borrowck failed");

    let visitor = IrLoweringVisitor::new();
    let ir = visitor.lower_ast(&transformed, "test_ssa");

    let mut assigned_regs = HashSet::new();
    for block in &ir.blocks {
        for op in &block.instructions {
            let dest_opt = match op {
                IrOp::AssignImm { dest, .. } => Some(*dest),
                IrOp::Alloc { dest, .. } => Some(*dest),
                IrOp::Load { dest, .. } => Some(*dest),
                IrOp::VolatileLoad { dest, .. } => Some(*dest),
                IrOp::Add { dest, .. } => Some(*dest),
                IrOp::Call { dest, .. } => *dest,
                IrOp::Store { .. } | IrOp::VolatileStore { .. } | IrOp::Free { .. } => None,
            };

            if let Some(dest) = dest_opt {
                assert!(assigned_regs.insert(dest.0), "Violación de Invariante SSA: Registro %{} asignado más de una vez", dest.0);
            }
        }
    }
}

#[test]
fn test_raw_ptr_outside_unsafe_rejected() {
    let code = r#"
    {
        let port: raw u32 = 65536;
    }
    "#;
    let lexer = Token::lexer(code);
    let tokens: Vec<Token> = lexer.filter_map(Result::ok).collect();
    let mut parser = Parser::new(tokens.into_iter());
    let ast = parser.parse_block().expect("Parse failed");

    let mut ctx = BorrowContext::new();
    let res = check_statement(&ast, &mut ctx);
    assert!(res.is_err(), "Vincular raw ptr fuera de unsafe debe fallar");
}

#[test]
fn test_q1_linear_branch_mismatch_rejected() {
    use sovereign_c::ast::{Expr, Stmt, Type};

    // Declaramos recurso afín `token`
    // En `then` se mueve `token`, pero en `else` no se consume (consumo asimétrico).
    let ast = Stmt::Block(vec![
        Stmt::Let("token".to_string(), Type::Owned(Box::new(Type::U8)), Expr::LiteralInt(64)),
        Stmt::If {
            cond: Expr::LiteralBool(true),
            then_branch: Box::new(Stmt::Block(vec![
                Stmt::Let("sink".to_string(), Type::Owned(Box::new(Type::U8)), Expr::Move("token".to_string())),
            ])),
            else_branch: Some(Box::new(Stmt::Block(vec![]))),
        },
    ]);

    let mut ctx = BorrowContext::new();
    let res = check_statement(&ast, &mut ctx);
    assert!(
        res.is_err(),
        "Regla Q1: El consumo asimétrico de variable afín en ramas condicionales debe fallar cerrado"
    );
    let err = res.err().unwrap();
    assert!(
        err.contains("LinearVariableMismatch"),
        "El error debe identificar LinearVariableMismatch: {}",
        err
    );
}

#[test]
fn test_q1_symmetric_linear_branch_accepted_and_lowered() {
    use sovereign_c::ast::{Expr, Stmt, Type};

    // Ambas ramas consumen `token` simétricamente (u_t = u_e = 1)
    let ast = Stmt::Block(vec![
        Stmt::Let("token".to_string(), Type::Owned(Box::new(Type::U8)), Expr::LiteralInt(64)),
        Stmt::If {
            cond: Expr::LiteralBool(true),
            then_branch: Box::new(Stmt::Block(vec![
                Stmt::Let("sink1".to_string(), Type::Owned(Box::new(Type::U8)), Expr::Move("token".to_string())),
            ])),
            else_branch: Some(Box::new(Stmt::Block(vec![
                Stmt::Let("sink2".to_string(), Type::Owned(Box::new(Type::U8)), Expr::Move("token".to_string())),
            ]))),
        },
    ]);

    let mut ctx = BorrowContext::new();
    let transformed = check_statement(&ast, &mut ctx).expect("El consumo simétrico debe compilar");
    let visitor = IrLoweringVisitor::new();
    let ir = visitor.lower_ast(&transformed, "q1_symmetric");
    assert!(!ir.blocks.is_empty(), "IR debe generar bloques básicos para bifurcación y unión");
}
