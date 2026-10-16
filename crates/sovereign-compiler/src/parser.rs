use crate::lexer::Token;
use crate::ast::{Stmt, Expr, Type};
use std::iter::Peekable;

pub struct Parser<I: Iterator<Item = Token>> {
    tokens: Peekable<I>,
}

impl<I: Iterator<Item = Token>> Parser<I> {
    pub fn new(tokens: I) -> Self {
        Self { tokens: tokens.peekable() }
    }

    /// Comprime un flujo de tokens en el invariante topológico (Block)
    pub fn parse_block(&mut self) -> Result<Stmt, String> {
        let mut stmts = Vec::new();
        
        if self.tokens.next() != Some(Token::LBrace) {
            return Err("Se esperaba '{' para abrir el bloque léxico".to_string());
        }
        
        while let Some(tok) = self.tokens.peek() {
            if tok == &Token::RBrace {
                self.tokens.next();
                break;
            }
            stmts.push(self.parse_stmt()?);
        }
        
        Ok(Stmt::Block(stmts))
    }

    fn parse_stmt(&mut self) -> Result<Stmt, String> {
        match self.tokens.next() {
            Some(Token::Unsafe) => {
                let block = self.parse_block()?;
                if let Stmt::Block(stmts) = block {
                    Ok(Stmt::UnsafeBlock(stmts))
                } else {
                    Err("Fallo topológico: se esperaba un bloque léxico tras 'unsafe'".to_string())
                }
            }
            Some(Token::Let) => {
                let name = match self.tokens.next() {
                    Some(Token::Ident(n)) => n,
                    _ => return Err("Se esperaba un identificador tras 'let'".to_string()),
                };
                
                if self.tokens.next() != Some(Token::Colon) { return Err("Se esperaba ':'".to_string()); }
                let ty = self.parse_type()?;
                
                if self.tokens.next() != Some(Token::Assign) { return Err("Se esperaba '='".to_string()); }
                let expr = self.parse_expr()?;
                
                if self.tokens.next() != Some(Token::Semi) { return Err("Se esperaba ';'".to_string()); }
                
                Ok(Stmt::Let(name, ty, expr))
            }
            Some(Token::Star) => {
                let name = match self.tokens.next() {
                    Some(Token::Ident(n)) => n,
                    _ => return Err("Se esperaba un identificador tras '*'".to_string()),
                };
                if self.tokens.next() != Some(Token::Assign) {
                    return Err("Se esperaba '=' tras desreferenciación".to_string());
                }
                let expr = self.parse_expr()?;
                if self.tokens.next() != Some(Token::Semi) {
                    return Err("Se esperaba ';'".to_string());
                }
                Ok(Stmt::Assign(Expr::Deref(Box::new(Expr::Variable(name))), expr))
            }
            _ => Err("Sentencia no válida o no soportada".to_string()),
        }
    }

    fn parse_type(&mut self) -> Result<Type, String> {
        match self.tokens.next() {
            Some(Token::U8) => Ok(Type::U8),
            Some(Token::U32) => Ok(Type::U32),
            Some(Token::Owned) => {
                let inner = self.parse_type()?;
                Ok(Type::Owned(Box::new(inner)))
            }
            Some(Token::Raw) => {
                let inner = self.parse_type()?;
                Ok(Type::RawPtr(Box::new(inner)))
            }
            _ => Err("Tipo de dato no válido".to_string()),
        }
    }

    fn parse_expr(&mut self) -> Result<Expr, String> {
        match self.tokens.next() {
            Some(Token::Int(v)) => Ok(Expr::LiteralInt(v)),
            Some(Token::Ident(n)) => Ok(Expr::Variable(n)),
            _ => Err("Expresión no válida".to_string()),
        }
    }
}
