#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Lifetime {
    Static,
    Lexical(usize), // Identificador de scope
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    /// Tipos escalares puros (Copy semantics, baja entropía)
    U8,
    U32,
    U64,
    Bool,
    /// Puntero Propietario (Affine semantics: move-only)
    /// Garantiza liberación determinista (Drop) al salir de scope.
    Owned(Box<Type>),
    /// Préstamo inmutable (Shared, Read-only)
    Ref(Lifetime, Box<Type>),
    /// Préstamo mutable (Exclusive, Read-Write, no-alias)
    MutRef(Lifetime, Box<Type>),
    /// Escotilla de escape OS/Ring-0: Puntero crudo sin garantías estáticas.
    /// Solo utilizable en bloques `unsafe`.
    RawPtr(Box<Type>), 
}

impl Type {
    /// Determina si un tipo obedece a semántica Affine (Move) o si es trivialmente copiable (Copy).
    pub fn is_affine(&self) -> bool {
        match self {
            Type::Owned(_) | Type::MutRef(_, _) => true,
            Type::Ref(_, _) | Type::RawPtr(_) | Type::U8 | Type::U32 | Type::U64 | Type::Bool => false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Expr {
    LiteralInt(u64),
    LiteralBool(bool),
    Variable(String),
    /// Operación fundamental de transferencia de propiedad térmica.
    Move(String),
    /// Préstamo explícito
    Borrow(String),
    BorrowMut(String),
    /// Desreferencia (segura para Ref/MutRef, requiere Unsafe para RawPtr)
    Deref(Box<Expr>),
    /// Aritmética básica
    Add(Box<Expr>, Box<Expr>),
}

#[derive(Debug, Clone)]
pub enum Stmt {
    /// Enlace de variable léxica
    Let(String, Type, Expr),
    /// Reasignación (solo válida si hay exclusividad)
    Assign(Expr, Expr),
    /// Bloque léxico delimitador de Lifetimes
    Block(Vec<Stmt>),
    /// Bloque de anergía controlada (Ring-0 operations)
    UnsafeBlock(Vec<Stmt>),
    /// Nodo SINTÉTICO: Inyectado por el compilador para el Drop determinista.
    /// No escribible por el programador.
    SyntheticDrop(String),
    /// Retorno de función
    Return(Expr),
}

#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub args: Vec<(String, Type)>,
    pub return_type: Type,
    pub body: Stmt, // Esperablemente un Stmt::Block
}
