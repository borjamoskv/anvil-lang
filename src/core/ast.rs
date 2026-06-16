#![allow(dead_code)]
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Program {
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, Serialize)]
pub enum Item {
    Function(FnDef),
    Struct(StructDef),
    Const(ConstDef),
    Contract(ContractDef),
    /// Ghost variable — exists only in the proof domain, not compiled to silicon
    GhostVar(GhostVarDef),
}

#[derive(Debug, Clone, Serialize)]
pub struct ContractDef {
    pub name: String,
    pub state_vars: Vec<StateVar>,
    pub functions: Vec<FnDef>,
    pub invariants: Vec<Invariant>,
    pub span: Option<Span>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StateVar {
    pub name: String,
    pub ty: Type,
    pub default: Option<Expr>,
    pub span: Option<Span>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FnDef {
    pub name: String,
    pub is_pub: bool,
    pub params: Vec<Param>,
    pub return_type: Option<Type>,
    pub invariants: Vec<Invariant>,
    /// Environment axioms — trusted without proof (Frontera Determinista)
    pub assumes: Vec<Invariant>,
    pub body: Block,
    pub span: Option<Span>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Param {
    pub name: String,
    pub ty: Type,
    pub is_mut: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct StructDef {
    pub name: String,
    pub is_pub: bool,
    pub fields: Vec<Field>,
    pub span: Option<Span>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Field {
    pub name: String,
    pub ty: Type,
    pub is_pub: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConstDef {
    pub name: String,
    pub ty: Type,
    pub value: Expr,
    pub span: Option<Span>,
}

/// Ghost variable definition — exists only in the proof domain.
/// Not compiled to silicon. Used for auxiliary reasoning in Z3.
#[derive(Debug, Clone, Serialize)]
pub struct GhostVarDef {
    pub name: String,
    pub ty: Type,
    pub value: Expr,
    pub span: Option<Span>,
}

// THE CORE INNOVATION: Invariants as first-class AST nodes
#[derive(Debug, Clone, Serialize)]
pub struct Invariant {
    pub expr: InvariantExpr,
    pub span: Option<Span>,
}

#[derive(Debug, Clone, Serialize)]
pub enum InvariantExpr {
    Comparison {
        left: Box<InvTerm>,
        op: CmpOp,
        right: Box<InvTerm>,
    },
    And(Box<InvariantExpr>, Box<InvariantExpr>),
    Or(Box<InvariantExpr>, Box<InvariantExpr>),
    Not(Box<InvariantExpr>),
    Forall {
        var: String,
        domain: Box<InvTerm>,
        body: Box<InvariantExpr>,
    },
    Exists {
        var: String,
        domain: Box<InvTerm>,
        body: Box<InvariantExpr>,
    },
    True,
}

#[derive(Debug, Clone, Serialize)]
pub enum InvTerm {
    Var {
        name: String,
        is_post: bool,
    },
    Literal(i128),
    BigLiteral(String),
    BinOp {
        left: Box<InvTerm>,
        op: ArithOp,
        right: Box<InvTerm>,
    },
    FieldAccess {
        object: String,
        field: String,
        is_post: bool,
    },
    FnCall {
        name: String,
        args: Vec<InvTerm>,
    },
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum Type {
    U8,
    U16,
    U32,
    U64,
    U128,
    U256,
    I8,
    I16,
    I32,
    I64,
    I128,
    Bool,
    Address,
    String,
    Unit,
    // Sovereign on-chain primitives (Mandate 8: Dinero = Energía = Información)
    Wallet,    // 32-byte public key / account identifier
    Signature, // 65-byte ECDSA/EdDSA signature
    TxHash,    // 32-byte transaction hash
    Gas,       // u64-bounded execution cost unit
    Array(Box<Type>),
    Map(Box<Type>, Box<Type>),
    Option(Box<Type>),
    Result(Box<Type>, Box<Type>),
    Arena(usize),
    Named(std::string::String),
}

#[derive(Debug, Clone, Serialize)]
pub enum Expr {
    IntLit(i128),
    BigIntLit(String),
    FloatLit(f64),
    BoolLit(bool),
    StringLit(String),
    HexLit(String),
    AddressLit(String),
    Ident(String),
    BinOp {
        left: Box<Expr>,
        op: BinOp,
        right: Box<Expr>,
    },
    UnaryOp {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    FnCall {
        name: String,
        args: Vec<Expr>,
    },
    MethodCall {
        object: Box<Expr>,
        method: String,
        args: Vec<Expr>,
    },
    FieldAccess {
        object: Box<Expr>,
        field: String,
    },
    Index {
        object: Box<Expr>,
        index: Box<Expr>,
    },
    If {
        condition: Box<Expr>,
        then_block: Block,
        else_block: Option<Block>,
    },
    Block(Block),
    Alloc {
        arena: Box<Expr>,
        value: Box<Expr>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub expr: Option<Box<Expr>>,
}

#[derive(Debug, Clone, Serialize)]
pub enum Stmt {
    Let {
        name: String,
        ty: Option<Type>,
        is_mut: bool,
        value: Expr,
    },
    Assign {
        target: LValue,
        op: AssignOp,
        value: Expr,
    },
    If {
        condition: Expr,
        then_block: Block,
        else_block: Option<Block>,
    },
    While {
        condition: Expr,
        invariants: Vec<Invariant>,
        body: Block,
    },
    Return(Option<Expr>),
    Assert {
        condition: Expr,
        message: Option<String>,
    },
    /// On-chain event emission (compiled to LOG opcode or equivalent)
    Emit {
        event: String,
        args: Vec<Expr>,
    },
    /// Ghost statement — proof-only binding, stripped from codegen
    Ghost {
        name: String,
        ty: Type,
        value: Expr,
    },
    Expr(Expr),
}

#[derive(Debug, Clone, Serialize)]
pub enum LValue {
    Ident(String),
    FieldAccess { object: String, field: String },
    Index { object: String, index: Box<Expr> },
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Neq,
    Lt,
    Gt,
    Lte,
    Gte,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum CmpOp {
    Eq,
    Neq,
    Lt,
    Gt,
    Lte,
    Gte,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum ArithOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum AssignOp {
    Assign,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
    BitAndAssign,
    BitOrAssign,
    BitXorAssign,
    ShlAssign,
    ShrAssign,
}
