// ============================================================================
// SOVEREIGN COMPILER - C5-REAL ARCHITECTURE
// █ BIFURCACIÓN 2: ONTOLOGÍA DE REPRESENTACIÓN INTERMEDIA (IR)
// █ FORMATO: SSA (Static Single Assignment) & THREE-ADDRESS CODE (TAC)
// ============================================================================

use std::fmt;

/// Identificador de Registro Virtual SSA (%0, %1, %2...)
/// Invariante: Asignación Estática Única. Inmutable tras su definición.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IrReg(pub usize);

impl fmt::Display for IrReg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "%{}", self.0)
    }
}

/// Operando en el plano de tres direcciones: puede ser un inmediato o un registro virtual
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operand {
    Reg(IrReg),
    Imm(u64),
}

impl fmt::Display for Operand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Operand::Reg(r) => write!(f, "{}", r),
            Operand::Imm(v) => write!(f, "{}", v),
        }
    }
}

/// Operaciones elementales de la IR SSA
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrOp {
    /// Asignación escalar inmediata: dest = imm
    AssignImm {
        dest: IrReg,
        value: u64,
    },
    
    /// Reserva de memoria delegada al kernel (Nacimiento de recurso Owned): dest = alloc(size)
    Alloc {
        dest: IrReg,
        size: u64,
    },
    
    /// Carga desde memoria estándar (sujeta a reordenamiento/DCE): dest = *ptr
    Load {
        dest: IrReg,
        ptr: IrReg,
    },
    
    /// Escritura estándar en memoria (sujeta a optimización): *ptr = value
    Store {
        ptr: IrReg,
        value: Operand,
    },
    
    /// Carga volátil de hardware (Ring-0 / MMIO): dest = volatile_load(*ptr)
    /// Inmune a Dead Code Elimination y reordenamiento de memoria.
    VolatileLoad {
        dest: IrReg,
        ptr: IrReg,
    },
    
    /// Escritura volátil de hardware (Ring-0 / MMIO): volatile_store(*ptr, value)
    /// Barrera física para controladores y puertos periféricos.
    VolatileStore {
        ptr: IrReg,
        value: Operand,
    },
    
    /// Destilación formal del SyntheticDrop: free(ptr)
    /// Emite la llamada canónica a @__sovereign_dealloc
    Free {
        ptr: IrReg,
    },
    
    /// Operación binaria aritmética: dest = lhs + rhs
    Add {
        dest: IrReg,
        lhs: Operand,
        rhs: Operand,
    },
    
    /// Llamada a función o símbolo externo: dest = call target(args)
    Call {
        dest: Option<IrReg>,
        target: String,
        args: Vec<Operand>,
    },
}

impl fmt::Display for IrOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IrOp::AssignImm { dest, value } => {
                write!(f, "  {} = imm {}", dest, value)
            }
            IrOp::Alloc { dest, size } => {
                write!(f, "  {} = alloc {} bytes", dest, size)
            }
            IrOp::Load { dest, ptr } => {
                write!(f, "  {} = load [{}]", dest, ptr)
            }
            IrOp::Store { ptr, value } => {
                write!(f, "  store [{}], {}", ptr, value)
            }
            IrOp::VolatileLoad { dest, ptr } => {
                write!(f, "  {} = volatile_load [{}] ; [MMIO BARRIER]", dest, ptr)
            }
            IrOp::VolatileStore { ptr, value } => {
                write!(f, "  volatile_store [{}], {} ; [MMIO BARRIER]", ptr, value)
            }
            IrOp::Free { ptr } => {
                write!(f, "  free {} ; [@__sovereign_dealloc]", ptr)
            }
            IrOp::Add { dest, lhs, rhs } => {
                write!(f, "  {} = add {}, {}", dest, lhs, rhs)
            }
            IrOp::Call { dest, target, args } => {
                let args_str: Vec<String> = args.iter().map(|a| a.to_string()).collect();
                if let Some(d) = dest {
                    write!(f, "  {} = call {}({})", d, target, args_str.join(", "))
                } else {
                    write!(f, "  call {}({})", target, args_str.join(", "))
                }
            }
        }
    }
}

/// Terminador estricto de Bloque Básico en el Grafo de Flujo de Control (CFG)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Terminator {
    /// Salto incondicional a otro bloque
    Jump(String),
    /// Salto condicional
    BranchIf {
        cond: IrReg,
        then_bb: String,
        else_bb: String,
    },
    /// Retorno de función
    Return(Option<Operand>),
    /// Detención soberana de la CPU / Kernel Halt
    Halt,
}

impl fmt::Display for Terminator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Terminator::Jump(target) => write!(f, "  jmp {}", target),
            Terminator::BranchIf { cond, then_bb, else_bb } => {
                write!(f, "  branch_if {}, then: {}, else: {}", cond, then_bb, else_bb)
            }
            Terminator::Return(Some(op)) => write!(f, "  ret {}", op),
            Terminator::Return(None) => write!(f, "  ret"),
            Terminator::Halt => write!(f, "  halt ; [Ring-0 Panic / Halt]"),
        }
    }
}

/// Bloque Básico (Basic Block): Secuencia lineal de instrucciones con entrada única y salida unívoca
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasicBlock {
    pub label: String,
    pub instructions: Vec<IrOp>,
    pub terminator: Terminator,
}

impl fmt::Display for BasicBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}:", self.label)?;
        for inst in &self.instructions {
            writeln!(f, "{}", inst)?;
        }
        writeln!(f, "{}", self.terminator)
    }
}

/// Grafo de Flujo de Control completo de una función o unidad de compilación
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionIr {
    pub name: String,
    pub blocks: Vec<BasicBlock>,
}

impl fmt::Display for FunctionIr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "fn @{}() {{", self.name)?;
        for (i, block) in self.blocks.iter().enumerate() {
            write!(f, "{}", block)?;
            if i + 1 < self.blocks.len() {
                writeln!(f)?;
            }
        }
        writeln!(f, "}}")
    }
}
