// ============================================================================
// SOVEREIGN COMPILER - C5-REAL ARCHITECTURE
// █ BIFURCACIÓN 3: ANÁLISIS DE VIVACIDAD (LIVENESS ANALYSIS)
// ============================================================================

use std::collections::HashMap;
use crate::ir::{FunctionIr, IrReg, IrOp, Operand, Terminator};

/// Intervalo de vida de un registro virtual SSA [%start, %end]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveInterval {
    pub reg: IrReg,
    pub start: usize,
    pub end: usize,
}

/// Computa los intervalos de vida de todos los registros virtuales SSA en la función.
/// Los intervalos se devuelven ordenados ascendentemente por su punto de inicio (`start`).
pub fn compute_live_intervals(func: &FunctionIr) -> Vec<LiveInterval> {
    let mut defs: HashMap<IrReg, usize> = HashMap::new();
    let mut last_uses: HashMap<IrReg, usize> = HashMap::new();
    
    let mut pc: usize = 0;

    for block in &func.blocks {
        for inst in &block.instructions {
            // Recoger usos de la instrucción
            let uses = get_instruction_uses(inst);
            for u in uses {
                last_uses.insert(u, pc);
            }

            // Recoger definición de la instrucción
            if let Some(d) = get_instruction_def(inst) {
                defs.entry(d).or_insert(pc);
                // Asegurar que si una variable nunca se usa, su intervalo es al menos [pc, pc]
                last_uses.entry(d).or_insert(pc);
            }

            pc += 1;
        }

        // Recoger usos del terminador del bloque
        let term_uses = get_terminator_uses(&block.terminator);
        for u in term_uses {
            last_uses.insert(u, pc);
        }
        pc += 1;
    }

    let mut intervals: Vec<LiveInterval> = defs
        .into_iter()
        .map(|(reg, start)| {
            let end = *last_uses.get(&reg).unwrap_or(&start);
            // El intervalo debe ser al menos [start, start]
            let normalized_end = if end < start { start } else { end };
            LiveInterval {
                reg,
                start,
                end: normalized_end,
            }
        })
        .collect();

    // Ordenar por punto de definición para el barrido lineal
    intervals.sort_by_key(|inv| inv.start);
    intervals
}

fn get_instruction_def(op: &IrOp) -> Option<IrReg> {
    match op {
        IrOp::AssignImm { dest, .. } => Some(*dest),
        IrOp::Alloc { dest, .. } => Some(*dest),
        IrOp::Load { dest, .. } => Some(*dest),
        IrOp::VolatileLoad { dest, .. } => Some(*dest),
        IrOp::Add { dest, .. } => Some(*dest),
        IrOp::Call { dest, .. } => *dest,
        IrOp::Store { .. } | IrOp::VolatileStore { .. } | IrOp::Free { .. } => None,
    }
}

fn get_instruction_uses(op: &IrOp) -> Vec<IrReg> {
    let mut uses = Vec::new();
    match op {
        IrOp::Store { ptr, value } | IrOp::VolatileStore { ptr, value } => {
            uses.push(*ptr);
            if let Operand::Reg(r) = value {
                uses.push(*r);
            }
        }
        IrOp::Load { ptr, .. } | IrOp::VolatileLoad { ptr, .. } | IrOp::Free { ptr } => {
            uses.push(*ptr);
        }
        IrOp::Add { lhs, rhs, .. } => {
            if let Operand::Reg(r) = lhs {
                uses.push(*r);
            }
            if let Operand::Reg(r) = rhs {
                uses.push(*r);
            }
        }
        IrOp::Call { args, .. } => {
            for arg in args {
                if let Operand::Reg(r) = arg {
                    uses.push(*r);
                }
            }
        }
        IrOp::AssignImm { .. } | IrOp::Alloc { .. } => {}
    }
    uses
}

fn get_terminator_uses(term: &Terminator) -> Vec<IrReg> {
    match term {
        Terminator::BranchIf { cond, .. } => vec![*cond],
        Terminator::Return(Some(Operand::Reg(r))) => vec![*r],
        _ => Vec::new(),
    }
}
