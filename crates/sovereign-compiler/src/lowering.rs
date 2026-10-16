// ============================================================================
// SOVEREIGN COMPILER - C5-REAL ARCHITECTURE
// █ BIFURCACIÓN 2: MOTOR DE LOWERING (AST -> SSA IR CFG)
// ============================================================================

use std::collections::HashMap;
use crate::ast::{Stmt, Expr, Type};
use crate::ir::{IrReg, Operand, IrOp, Terminator, BasicBlock, FunctionIr};

pub struct IrLoweringVisitor {
    /// Pila de tablas de símbolos léxicos: variable -> (registro SSA actual, Type)
    symbol_table: Vec<HashMap<String, (IrReg, Type)>>,
    /// Contador monotónico de registros virtuales SSA (%0, %1, %2...)
    next_reg_id: usize,
    /// Contador de bloques para etiquetas únicas
    block_counter: usize,
    /// Bloques básicos completados
    completed_blocks: Vec<BasicBlock>,
    /// Instrucciones del bloque en construcción
    current_instructions: Vec<IrOp>,
    /// Etiqueta del bloque activo
    current_label: String,
    /// Manta de Markov: indica si estamos dentro de un ámbito unsafe (Ring-0)
    in_unsafe: bool,
}

impl IrLoweringVisitor {
    pub fn new() -> Self {
        Self {
            symbol_table: vec![HashMap::new()],
            next_reg_id: 0,
            block_counter: 0,
            completed_blocks: Vec::new(),
            current_instructions: Vec::new(),
            current_label: "bb_entry".to_string(),
            in_unsafe: false,
        }
    }

    /// Asigna un nuevo registro SSA unívoco e inmutable (%0, %1, %2...)
    fn alloc_reg(&mut self) -> IrReg {
        let r = IrReg(self.next_reg_id);
        self.next_reg_id += 1;
        r
    }

    /// Genera una nueva etiqueta de bloque básico
    fn new_label(&mut self, prefix: &str) -> String {
        let lbl = format!("{}_{}", prefix, self.block_counter);
        self.block_counter += 1;
        lbl
    }

    /// Cierra el bloque actual con un terminador y prepara uno nuevo
    fn finish_block(&mut self, terminator: Terminator, next_label: String) {
        let bb = BasicBlock {
            label: std::mem::replace(&mut self.current_label, next_label),
            instructions: std::mem::take(&mut self.current_instructions),
            terminator,
        };
        self.completed_blocks.push(bb);
    }

    fn push_scope(&mut self) {
        self.symbol_table.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.symbol_table.pop();
    }

    fn insert_symbol(&mut self, name: String, reg: IrReg, ty: Type) {
        if let Some(scope) = self.symbol_table.last_mut() {
            scope.insert(name, (reg, ty));
        }
    }

    fn lookup_symbol(&self, name: &str) -> Option<(IrReg, Type)> {
        for scope in self.symbol_table.iter().rev() {
            if let Some(entry) = scope.get(name) {
                return Some(entry.clone());
            }
        }
        None
    }

    /// Punto de entrada principal para bajar un AST transformado a FunctionIr
    pub fn lower_ast(mut self, stmts: &[Stmt], fn_name: &str) -> FunctionIr {
        for stmt in stmts {
            self.lower_stmt(stmt);
        }

        // Si el último bloque no ha sido cerrado, se sella con Halt (Ring-0)
        let last_bb = BasicBlock {
            label: self.current_label,
            instructions: self.current_instructions,
            terminator: Terminator::Halt,
        };
        self.completed_blocks.push(last_bb);

        FunctionIr {
            name: fn_name.to_string(),
            blocks: self.completed_blocks,
        }
    }

    fn lower_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let(name, ty, expr) => {
                match ty {
                    Type::Owned(_) => {
                        let dest = self.alloc_reg();
                        let size = match expr {
                            Expr::LiteralInt(s) => *s,
                            _ => 64, // Default allocation size
                        };
                        self.current_instructions.push(IrOp::Alloc { dest, size });
                        self.insert_symbol(name.clone(), dest, ty.clone());
                    }
                    Type::RawPtr(_) => {
                        let dest = self.alloc_reg();
                        let addr = match expr {
                            Expr::LiteralInt(a) => *a,
                            _ => 0,
                        };
                        self.current_instructions.push(IrOp::AssignImm { dest, value: addr });
                        self.insert_symbol(name.clone(), dest, ty.clone());
                    }
                    _ => {
                        // Escalares puros (Copy)
                        let val_op = self.lower_expr(expr);
                        let dest = self.alloc_reg();
                        match val_op {
                            Operand::Imm(v) => {
                                self.current_instructions.push(IrOp::AssignImm { dest, value: v });
                            }
                            Operand::Reg(src) => {
                                self.current_instructions.push(IrOp::Add {
                                    dest,
                                    lhs: Operand::Reg(src),
                                    rhs: Operand::Imm(0),
                                });
                            }
                        }
                        self.insert_symbol(name.clone(), dest, ty.clone());
                    }
                }
            }
            Stmt::Assign(lhs, rhs) => {
                let rhs_op = self.lower_expr(rhs);
                if let Expr::Deref(inner) = lhs {
                    if let Expr::Variable(name) = &**inner {
                        if let Some((ptr_reg, ty)) = self.lookup_symbol(name) {
                            if matches!(ty, Type::RawPtr(_)) || self.in_unsafe {
                                // IMPERATIVO 3: Blindaje Volátil para Ring-0 MMIO
                                self.current_instructions.push(IrOp::VolatileStore {
                                    ptr: ptr_reg,
                                    value: rhs_op,
                                });
                            } else {
                                self.current_instructions.push(IrOp::Store {
                                    ptr: ptr_reg,
                                    value: rhs_op,
                                });
                            }
                        }
                    }
                } else if let Expr::Variable(name) = lhs {
                    // Reasignación SSA: nace un nuevo registro inmutable
                    let dest = self.alloc_reg();
                    match rhs_op {
                        Operand::Imm(v) => {
                            self.current_instructions.push(IrOp::AssignImm { dest, value: v });
                        }
                        Operand::Reg(src) => {
                            self.current_instructions.push(IrOp::Add {
                                dest,
                                lhs: Operand::Reg(src),
                                rhs: Operand::Imm(0),
                            });
                        }
                    }
                    if let Some((_, old_ty)) = self.lookup_symbol(name) {
                        self.insert_symbol(name.clone(), dest, old_ty);
                    }
                }
            }
            Stmt::Block(stmts) => {
                self.push_scope();
                for s in stmts {
                    self.lower_stmt(s);
                }
                self.pop_scope();
            }
            Stmt::UnsafeBlock(stmts) => {
                // Bifurcación topológica a Bloque Básico de Ring-0
                let unsafe_label = self.new_label("bb_unsafe");
                let post_label = self.new_label("bb_post_unsafe");

                self.finish_block(Terminator::Jump(unsafe_label.clone()), unsafe_label);
                
                self.push_scope();
                let prev_unsafe = self.in_unsafe;
                self.in_unsafe = true;

                for s in stmts {
                    self.lower_stmt(s);
                }

                self.in_unsafe = prev_unsafe;
                self.pop_scope();

                self.finish_block(Terminator::Jump(post_label.clone()), post_label);
            }
            Stmt::SyntheticDrop(name) => {
                // IMPERATIVO 2: Materialización del SyntheticDrop en Free (@__sovereign_dealloc)
                if let Some((ptr_reg, ty)) = self.lookup_symbol(name) {
                    if ty.is_affine() {
                        self.current_instructions.push(IrOp::Free { ptr: ptr_reg });
                    }
                }
            }
            Stmt::Return(expr) => {
                let op = self.lower_expr(expr);
                let next_label = self.new_label("bb_unreachable");
                self.finish_block(Terminator::Return(Some(op)), next_label);
            }
        }
    }

    fn lower_expr(&mut self, expr: &Expr) -> Operand {
        match expr {
            Expr::LiteralInt(v) => Operand::Imm(*v),
            Expr::LiteralBool(b) => Operand::Imm(if *b { 1 } else { 0 }),
            Expr::Variable(name) | Expr::Move(name) => {
                if let Some((reg, _)) = self.lookup_symbol(name) {
                    Operand::Reg(reg)
                } else {
                    panic!("Fallo interno de lowering: Símbolo no resuelto '{}'", name);
                }
            }
            Expr::Add(lhs, rhs) => {
                let lhs_op = self.lower_expr(lhs);
                let rhs_op = self.lower_expr(rhs);
                let dest = self.alloc_reg();
                self.current_instructions.push(IrOp::Add {
                    dest,
                    lhs: lhs_op,
                    rhs: rhs_op,
                });
                Operand::Reg(dest)
            }
            Expr::Deref(inner) => {
                let ptr_op = self.lower_expr(inner);
                let dest = self.alloc_reg();
                if let Operand::Reg(ptr_reg) = ptr_op {
                    if self.in_unsafe {
                        self.current_instructions.push(IrOp::VolatileLoad { dest, ptr: ptr_reg });
                    } else {
                        self.current_instructions.push(IrOp::Load { dest, ptr: ptr_reg });
                    }
                }
                Operand::Reg(dest)
            }
            _ => Operand::Imm(0),
        }
    }
}
