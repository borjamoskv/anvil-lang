// ============================================================================
// SOVEREIGN COMPILER - C5-REAL ARCHITECTURE
// █ BIFURCACIÓN 3: ASIGNACIÓN DE REGISTROS POR BARRIDO LINEAL (LINEAR SCAN)
// ============================================================================

use std::collections::{HashMap, VecDeque};
use std::fmt;
use crate::ir::IrReg;
use crate::liveness::LiveInterval;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PhysReg {
    Rax,
    Rcx,
    Rdx,
    Rsi,
    Rdi,
    R8,
    R9,
    R10,
    R11,
}

impl fmt::Display for PhysReg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PhysReg::Rax => write!(f, "rax"),
            PhysReg::Rcx => write!(f, "rcx"),
            PhysReg::Rdx => write!(f, "rdx"),
            PhysReg::Rsi => write!(f, "rsi"),
            PhysReg::Rdi => write!(f, "rdi"),
            PhysReg::R8 => write!(f, "r8"),
            PhysReg::R9 => write!(f, "r9"),
            PhysReg::R10 => write!(f, "r10"),
            PhysReg::R11 => write!(f, "r11"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegLocation {
    Reg(PhysReg),
    Spill(usize),
}

impl fmt::Display for RegLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RegLocation::Reg(r) => write!(f, "{}", r),
            RegLocation::Spill(s) => write!(f, "[stack_spill #{}]", s),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RegAssignment {
    pub mapping: HashMap<IrReg, RegLocation>,
    pub num_spills: usize,
}

pub struct LinearScanAllocator {
    available_regs: VecDeque<PhysReg>,
    active: Vec<(LiveInterval, PhysReg)>,
    mapping: HashMap<IrReg, RegLocation>,
    next_spill_slot: usize,
}

impl LinearScanAllocator {
    pub fn new(pool: &[PhysReg]) -> Self {
        Self {
            available_regs: pool.iter().copied().collect(),
            active: Vec::new(),
            mapping: HashMap::new(),
            next_spill_slot: 0,
        }
    }

    pub fn default_x86_64() -> Self {
        Self::new(&[
            PhysReg::Rax,
            PhysReg::Rcx,
            PhysReg::Rdx,
            PhysReg::Rsi,
            PhysReg::Rdi,
            PhysReg::R8,
            PhysReg::R9,
            PhysReg::R10,
            PhysReg::R11,
        ])
    }

    pub fn allocate(mut self, mut intervals: Vec<LiveInterval>) -> RegAssignment {
        // Ordenar por inicio de intervalo ascendente
        intervals.sort_by_key(|inv| inv.start);

        for interval in intervals {
            self.expire_old_intervals(interval.start);

            if self.available_regs.is_empty() {
                self.spill_at_interval(interval);
            } else {
                let phys = self.available_regs.pop_front().unwrap();
                self.mapping.insert(interval.reg, RegLocation::Reg(phys));
                self.active.push((interval, phys));
                // Mantener activos ordenados por punto final ascendente
                self.active.sort_by_key(|(inv, _)| inv.end);
            }
        }

        RegAssignment {
            mapping: self.mapping,
            num_spills: self.next_spill_slot,
        }
    }

    fn expire_old_intervals(&mut self, current_start: usize) {
        let mut i = 0;
        while i < self.active.len() {
            if self.active[i].0.end < current_start {
                let (_, phys) = self.active.remove(i);
                self.available_regs.push_front(phys);
            } else {
                i += 1;
            }
        }
    }

    fn spill_at_interval(&mut self, interval: LiveInterval) {
        // Encontrar el intervalo con mayor tiempo de vida restante en `active`
        if let Some(last_active) = self.active.last() {
            if last_active.0.end > interval.end {
                // Expulsar el intervalo más largo a spill y ceder su registro físico
                let (spilled_interval, stolen_phys) = self.active.pop().unwrap();
                let spill_slot = self.next_spill_slot;
                self.next_spill_slot += 1;

                self.mapping.insert(spilled_interval.reg, RegLocation::Spill(spill_slot));
                self.mapping.insert(interval.reg, RegLocation::Reg(stolen_phys));

                self.active.push((interval, stolen_phys));
                self.active.sort_by_key(|(inv, _)| inv.end);
                return;
            }
        }

        // Si no hay candidatos mejores, spill del intervalo actual
        let spill_slot = self.next_spill_slot;
        self.next_spill_slot += 1;
        self.mapping.insert(interval.reg, RegLocation::Spill(spill_slot));
    }
}
