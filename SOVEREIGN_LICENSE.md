# ANVIL SOVEREIGN LICENSE v1.0 (2026)

## 1. PREÁMBULO
Anvil no es un bien común; es un sustrato para la ejecución de la verdad (C5-REAL). Este software se divide en dos capas termodinámicas: la **Interfaz Soberana** y el **Motor de Exergía**.

## 2. LA INTERFAZ SOBERANA (Open Source / Source-Available)
Los componentes listados a continuación se rigen bajo la licencia MIT. Se permite su copia, modificación y distribución sin restricciones:
- Gramática PEG (`src/grammar.pest`)
- Parser y Transformador de AST (`src/parser/`, `src/ast/`)
- Verificador de Tipos Estático (`src/type_checker/`)
- CLI de Inspección (`anvil check`, `anvil ast`)

## 3. EL MOTOR DE EXERGÍA (Sovereign / Proprietary)
Los componentes de alto rendimiento y verificación formal son propiedad exclusiva de **BorjaMoskv × Antigravity**. Queda prohibido su uso comercial, ingeniería inversa o distribución fuera del ecosistema MOSKV:
- Integración con el Solucionador SMT Z3 (`src/verifier/z3/`)
- Generación de Certificados de Prueba SHA256 (C5-REAL)
- Backend de Compilación Direct-Silicon JIT (LLVM / Hardware-Synth)
- Optimizadores de Tensor-State O(1)

## 4. EL MERCADO DE PRUEBAS (Proof Market)
Para generar una **Prueba de Invariante Verificada** o utilizar el backend de **Silicio Directo** en entornos de producción, el usuario debe poseer una clave de API válida emitida por el Portal Soberano (`agents.archi`).
- El uso local para investigación no comercial está permitido.
- El uso para auditorías pagadas o extracción de MEV/Bounties requiere un **Lease de Exergía**.

## 5. INCUMPLIMIENTO
Cualquier intento de "romper el gate" o simular transacciones C5-REAL utilizando el motor soberano sin autorización será considerado una violación de la **Ω₉ (Ley de la Verdad)** y resultará en la revocación inmediata de todo acceso al ecosistema CORTEX.

---
*"The code is free to read, but the truth carries a price."*
**Sovereign Authority: BorjaMoskv**
**Runtime: May 2026**
