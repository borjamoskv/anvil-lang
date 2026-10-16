// ============================================================================
// SOVEREIGN COMPILER - C5-REAL ARCHITECTURE
// █ BIFURCACIÓN 3: BACKEND DE EMISIÓN DE CÓDIGO CON REGISTROS FÍSICOS (x86_64)
// ============================================================================

use crate::ir::{FunctionIr, BasicBlock, IrOp, Terminator, Operand};
use crate::regalloc::{RegAssignment, RegLocation, PhysReg};

/// Transductor Termodinámico: CFG SSA IR + RegAlloc -> Código Máquina Soberano (x86_64)
pub fn generate_x86_64_optimized(func: &FunctionIr, alloc: &RegAssignment) -> String {
    let mut asm = String::new();
    
    // Cabecera Ring-0 (no_std)
    asm.push_str("; ==========================================\n");
    asm.push_str("; SOVEREIGN COMPILER - C5-REAL ARCHITECTURE\n");
    asm.push_str("; TARGET: x86_64 Bare-Metal (Ring-0)\n");
    asm.push_str(&format!("; FUNCTION: @{}\n", func.name));
    asm.push_str(&format!("; ALLOCATION: {} Spills requeridos\n", alloc.num_spills));
    asm.push_str("; ==========================================\n\n");
    asm.push_str(".global _start\n");
    asm.push_str(".section .text\n\n");
    
    asm.push_str("_start:\n");
    asm.push_str("    ; --- Prólogo del Kernel ---\n");
    asm.push_str("    push rbp\n");
    asm.push_str("    mov rbp, rsp\n");
    if alloc.num_spills > 0 {
        let frame_size = ((alloc.num_spills * 8 + 15) / 16) * 16;
        asm.push_str(&format!("    sub rsp, {} ; Espacio exclusivo para spills\n\n", frame_size));
    } else {
        asm.push_str("    ; [ALTA EXERGÍA: 0 SPILLS - Pila intacta]\n\n");
    }
    
    for bb in &func.blocks {
        emit_basic_block(bb, alloc, &mut asm);
    }
    
    asm
}

fn spill_offset(slot: usize) -> usize {
    (slot + 1) * 8
}

fn emit_basic_block(bb: &BasicBlock, alloc: &RegAssignment, asm: &mut String) {
    asm.push_str(&format!(".{}:\n", bb.label));
    
    for inst in &bb.instructions {
        match inst {
            IrOp::AssignImm { dest, value } => {
                let loc = alloc.mapping.get(dest).unwrap();
                asm.push_str(&format!("    ; SSA {} = Imm {}\n", dest, value));
                match loc {
                    RegLocation::Reg(r) => {
                        asm.push_str(&format!("    mov {}, {}\n", r, value));
                    }
                    RegLocation::Spill(slot) => {
                        asm.push_str(&format!("    mov qword ptr [rbp - {}], {}\n", spill_offset(*slot), value));
                    }
                }
            }
            IrOp::Alloc { dest, size } => {
                let loc = alloc.mapping.get(dest).unwrap();
                asm.push_str(&format!("    ; SSA {} = Alloc {} bytes [@__sovereign_alloc]\n", dest, size));
                asm.push_str(&format!("    mov rdi, {}\n", size));
                asm.push_str("    call sys_alloc_os\n");
                match loc {
                    RegLocation::Reg(PhysReg::Rax) => {
                        asm.push_str("    ; [Zero-Cost Transfer: Puntero devuelto en rax]\n");
                    }
                    RegLocation::Reg(r) => {
                        asm.push_str(&format!("    mov {}, rax\n", r));
                    }
                    RegLocation::Spill(slot) => {
                        asm.push_str(&format!("    mov qword ptr [rbp - {}], rax\n", spill_offset(*slot)));
                    }
                }
            }
            IrOp::Load { dest, ptr } => {
                let dest_loc = alloc.mapping.get(dest).unwrap();
                let ptr_loc = alloc.mapping.get(ptr).unwrap();
                asm.push_str(&format!("    ; SSA {} = Load [{}]\n", dest, ptr));
                let ptr_reg = load_reg_to_scratch(ptr_loc, "rsi", asm);
                match dest_loc {
                    RegLocation::Reg(r) => {
                        asm.push_str(&format!("    mov {}, qword ptr [{}]\n", r, ptr_reg));
                    }
                    RegLocation::Spill(slot) => {
                        asm.push_str(&format!("    mov rax, qword ptr [{}]\n", ptr_reg));
                        asm.push_str(&format!("    mov qword ptr [rbp - {}], rax\n", spill_offset(*slot)));
                    }
                }
            }
            IrOp::Store { ptr, value } => {
                let ptr_loc = alloc.mapping.get(ptr).unwrap();
                asm.push_str(&format!("    ; Store [{}], {}\n", ptr, value));
                let val_reg = load_operand_to_reg(value, alloc, "rax", asm);
                let ptr_reg = load_reg_to_scratch(ptr_loc, "rsi", asm);
                asm.push_str(&format!("    mov qword ptr [{}], {}\n", ptr_reg, val_reg));
            }
            IrOp::VolatileLoad { dest, ptr } => {
                let dest_loc = alloc.mapping.get(dest).unwrap();
                let ptr_loc = alloc.mapping.get(ptr).unwrap();
                asm.push_str(&format!("    ; [BARRIER: VOLATILE LOAD RING-0 MMIO] SSA {} = Load [{}]\n", dest, ptr));
                let ptr_reg = load_reg_to_scratch(ptr_loc, "rsi", asm);
                match dest_loc {
                    RegLocation::Reg(r) => {
                        asm.push_str(&format!("    mov {}, qword ptr [{}] ; Lectura atómica incondicional\n", r, ptr_reg));
                    }
                    RegLocation::Spill(slot) => {
                        asm.push_str(&format!("    mov rax, qword ptr [{}]\n", ptr_reg));
                        asm.push_str(&format!("    mov qword ptr [rbp - {}], rax\n", spill_offset(*slot)));
                    }
                }
            }
            IrOp::VolatileStore { ptr, value } => {
                let ptr_loc = alloc.mapping.get(ptr).unwrap();
                asm.push_str("    ; [BARRIER: VOLATILE STORE RING-0 MMIO - IMMUNE TO DCE]\n");
                let val_reg = load_operand_to_reg(value, alloc, "rax", asm);
                let ptr_reg = load_reg_to_scratch(ptr_loc, "rsi", asm);
                asm.push_str(&format!("    mov qword ptr [{}], {} ; Escritura física directa al bus\n", ptr_reg, val_reg));
                asm.push_str("    mfence ; Barrera de serialización estricta de memoria\n");
            }
            IrOp::Free { ptr } => {
                let ptr_loc = alloc.mapping.get(ptr).unwrap();
                asm.push_str(&format!("    ; [DEALLOC DETERMINISTA MATERIALIZADO: @__sovereign_dealloc({})]\n", ptr));
                match ptr_loc {
                    RegLocation::Reg(PhysReg::Rdi) => {
                        // Ya está en rdi
                    }
                    RegLocation::Reg(r) => {
                        asm.push_str(&format!("    mov rdi, {}\n", r));
                    }
                    RegLocation::Spill(slot) => {
                        asm.push_str(&format!("    mov rdi, qword ptr [rbp - {}]\n", spill_offset(*slot)));
                    }
                }
                asm.push_str("    call sys_free_os\n");
            }
            IrOp::Add { dest, lhs, rhs } => {
                let dest_loc = alloc.mapping.get(dest).unwrap();
                asm.push_str(&format!("    ; SSA {} = Add {}, {}\n", dest, lhs, rhs));
                let lhs_reg = load_operand_to_reg(lhs, alloc, "rax", asm);
                match dest_loc {
                    RegLocation::Reg(d) => {
                        if d.to_string() != lhs_reg {
                            asm.push_str(&format!("    mov {}, {}\n", d, lhs_reg));
                        }
                        match rhs {
                            Operand::Imm(v) => asm.push_str(&format!("    add {}, {}\n", d, v)),
                            Operand::Reg(r) => {
                                let r_loc = alloc.mapping.get(r).unwrap();
                                match r_loc {
                                    RegLocation::Reg(phys) => asm.push_str(&format!("    add {}, {}\n", d, phys)),
                                    RegLocation::Spill(slot) => asm.push_str(&format!("    add {}, qword ptr [rbp - {}]\n", d, spill_offset(*slot))),
                                }
                            }
                        }
                    }
                    RegLocation::Spill(slot) => {
                        asm.push_str(&format!("    mov r11, {}\n", lhs_reg));
                        match rhs {
                            Operand::Imm(v) => asm.push_str(&format!("    add r11, {}\n", v)),
                            Operand::Reg(r) => {
                                let r_loc = alloc.mapping.get(r).unwrap();
                                match r_loc {
                                    RegLocation::Reg(phys) => asm.push_str(&format!("    add r11, {}\n", phys)),
                                    RegLocation::Spill(s) => asm.push_str(&format!("    add r11, qword ptr [rbp - {}]\n", spill_offset(*s))),
                                }
                            }
                        }
                        asm.push_str(&format!("    mov qword ptr [rbp - {}], r11\n", spill_offset(*slot)));
                    }
                }
            }
            IrOp::Call { dest, target, args } => {
                asm.push_str(&format!("    ; Call {}({:?})\n", target, args));
                asm.push_str(&format!("    call {}\n", target));
                if let Some(d) = dest {
                    let d_loc = alloc.mapping.get(d).unwrap();
                    match d_loc {
                        RegLocation::Reg(PhysReg::Rax) => {}
                        RegLocation::Reg(r) => asm.push_str(&format!("    mov {}, rax\n", r)),
                        RegLocation::Spill(slot) => asm.push_str(&format!("    mov qword ptr [rbp - {}], rax\n", spill_offset(*slot))),
                    }
                }
            }
        }
    }

    match &bb.terminator {
        Terminator::Jump(target) => {
            asm.push_str(&format!("    jmp .{}\n\n", target));
        }
        Terminator::BranchIf { cond, then_bb, else_bb } => {
            let cond_loc = alloc.mapping.get(cond).unwrap();
            match cond_loc {
                RegLocation::Reg(r) => asm.push_str(&format!("    test {}, {}\n", r, r)),
                RegLocation::Spill(slot) => {
                    asm.push_str(&format!("    cmp qword ptr [rbp - {}], 0\n", spill_offset(*slot)));
                }
            }
            asm.push_str(&format!("    jnz .{}\n", then_bb));
            asm.push_str(&format!("    jmp .{}\n\n", else_bb));
        }
        Terminator::Return(Some(op)) => {
            let _ = load_operand_to_reg(op, alloc, "rax", asm);
            asm.push_str("    mov rsp, rbp\n");
            asm.push_str("    pop rbp\n");
            asm.push_str("    ret\n\n");
        }
        Terminator::Return(None) => {
            asm.push_str("    mov rsp, rbp\n");
            asm.push_str("    pop rbp\n");
            asm.push_str("    ret\n\n");
        }
        Terminator::Halt => {
            asm.push_str("    ; --- Kernel Panic / Halt Loop (Ring-0) ---\n");
            asm.push_str(&format!(".{}_halt:\n", bb.label));
            asm.push_str("    cli\n");
            asm.push_str("    hlt\n");
            asm.push_str(&format!("    jmp .{}_halt\n\n", bb.label));
        }
    }
}

fn load_reg_to_scratch(loc: &RegLocation, scratch: &str, asm: &mut String) -> String {
    match loc {
        RegLocation::Reg(r) => r.to_string(),
        RegLocation::Spill(slot) => {
            asm.push_str(&format!("    mov {}, qword ptr [rbp - {}]\n", scratch, spill_offset(*slot)));
            scratch.to_string()
        }
    }
}

fn load_operand_to_reg(op: &Operand, alloc: &RegAssignment, fallback_scratch: &str, asm: &mut String) -> String {
    match op {
        Operand::Imm(v) => {
            asm.push_str(&format!("    mov {}, {}\n", fallback_scratch, v));
            fallback_scratch.to_string()
        }
        Operand::Reg(r) => {
            let loc = alloc.mapping.get(r).unwrap();
            match loc {
                RegLocation::Reg(phys) => phys.to_string(),
                RegLocation::Spill(slot) => {
                    asm.push_str(&format!("    mov {}, qword ptr [rbp - {}]\n", fallback_scratch, spill_offset(*slot)));
                    fallback_scratch.to_string()
                }
            }
        }
    }
}

/// Transductor de compatibilidad que computa liveness + regalloc automáticamente
pub fn generate_x86_64_from_ir(func: &FunctionIr) -> String {
    let intervals = crate::liveness::compute_live_intervals(func);
    let allocator = crate::regalloc::LinearScanAllocator::default_x86_64();
    let assignment = allocator.allocate(intervals);
    generate_x86_64_optimized(func, &assignment)
}
