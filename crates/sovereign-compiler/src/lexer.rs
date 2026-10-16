use logos::Logos;

#[derive(Logos, Debug, PartialEq, Clone)]
#[logos(skip r"[ \t\n\f]+")] // Ignorar entropía (espacios en blanco)
pub enum Token {
    // Palabras clave
    #[token("let")] Let,
    #[token("owned")] Owned,
    #[token("raw")] Raw,
    #[token("unsafe")] Unsafe,
    #[token("u8")] U8,
    #[token("u32")] U32,
    
    // Símbolos
    #[token("=")] Assign,
    #[token(":")] Colon,
    #[token(";")] Semi,
    #[token("{")] LBrace,
    #[token("}")] RBrace,
    #[token("*")] Star,
    
    // Identificadores (Extracción Semántica)
    #[regex("[a-zA-Z_][a-zA-Z0-9_]*", |lex| lex.slice().to_string())]
    Ident(String),

    // Literales enteros
    #[regex("[0-9]+", |lex| lex.slice().parse::<u64>().unwrap())]
    Int(u64),
}
