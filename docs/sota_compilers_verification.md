# ESTADO DEL ARTE: DIRECT-SILICON VERIFICATION & SMART CONTRACT JIT COMPILERS
**Reality Level:** C5-REAL (Academic & Empirical SOTA verification)  
**Date:** June 2026  
**Context:** Anvil compiler integration  

## 1. Delimitación Temporal (2020 - 2026)
La investigación se delimita a los últimos 6 años para compilación y verificación de bajo nivel (eBPF, RISC-V) y 3 años para técnicas emergentes de compilación con SMT-solving nativo para entornos de ejecución de confianza cero (Zero-Trust execution).

---

## 2. Matriz Analítica SOTA

| Autor | Año | Objetivos | Metodología | Resultados | Conclusiones |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Nelson et al. (Jitterbug)** | 2020 | Verificar la corrección de compiladores JIT en el kernel de Linux. | Elevación semántica automática vía Rosette (SMT Z3) de intérpretes a verificadores de equivalencia de instrucciones. | Diseño y prueba completa de un compilador BPF JIT para RV32G; detección de 16 bugs latentes en el kernel. | La verificación del compilador JIT previene la generación de código incorrecto por el backend, pero no valida la lógica del programa del usuario. |
| **Vishwanathan et al. (Agni)** | 2023 | Verificar la corrección del análisis de rangos (range analysis) en el verificador eBPF de Linux. | Traducción automatizada del código C del verificador a lógica SMT-LIB2 alimentada a Z3. | Identificación y parcheo de múltiples unsoundness latentes en el seguimiento de límites de bits y offsets de memoria. | El análisis estático en tiempo de ejecución del kernel es frágil y costoso; verificar el analizador es vital para evitar desvíos del sandbox. |
| **Wu et al. (VEP)** | 2025 | Habilitar programabilidad total en eBPF desacoplando la verificación del runtime del kernel. | Arquitectura de dos fases: Verificador anotado en C (VEP-C) -> Compilador guiado -> Checker ligero de pruebas a nivel de bytecode (VEP-eBPF). | Verificación de programas complejos fuera del kernel con sobrecarga mínima de carga en runtime (<0.1ms). | Desacoplar la verificación reduce drásticamente el tamaño del Trusted Computing Base (TCB) en runtime. |
| **Solidity Team (SMTChecker)** | 2024 | Probar invariantes de Smart Contracts escritos en Solidity en tiempo de compilación. | Codificación de la semántica de Solidity a restricciones Horn (CHC) resueltas mediante Z3 y Eldarica. | Detección automatizada de overflow, reentrancy y aserciones violadas en Solidity. | Valida la lógica de negocio pero el código resultante corre en la EVM, disipando exergía masiva en runtime (runtime checks). |
| **SPLASH/OOPSLA (VeRefine)** | 2025 | Evitar fallos de compilación crípticos en eBPF usando tipos de refinamiento. | Sistema de tipos de refinamiento sensibles al flujo (flow-sensitive) con inferencia automática. | Reducción del ciclo de feedback del desarrollador ante fallos de verificación; reemplazo parcial de validadores dinámicos. | Los tipos de refinamiento reducen el esfuerzo de anotación manual pero requieren integración nativa en el compilador. |

---

## 3. Biopsia Crítica: Mecanismo Base vs. Vacío Exérgico

### A. Jitterbug & Serval (2020)
*   **Mecanismo Base:** Traduce la especificación del ISA origen (eBPF) y destino (RISC-V/x86) a fórmulas booleanas en Z3 para certificar equivalencia semántica de cada instrucción emitida por el JIT.
*   **Vacío Exérgico:** La verificación se detiene en el compilador. Si el programa original contiene errores lógicos o desbordamientos lógicos del contrato (ej. Double spend), Jitterbug garantiza que se compilarán fielmente al binario físico, manteniendo la vulnerabilidad de negocio intacta.

### B. VEP (2025)
*   **Mecanismo Base:** Utiliza anotaciones en C para guiar un verificador estático y produce pruebas de bytecode comprobables en O(1) por el kernel durante la carga.
*   **Vacío Exérgico:** La sintaxis es intrusiva. Obliga al desarrollador a escribir aserciones y pre/postcondiciones en el lenguaje C del host sin abstracciones del dominio del Smart Contract (Wallet, Gas, TxHash), generando un alto esfuerzo cognitivo.

### C. Solidity SMTChecker (2024)
*   **Mecanismo Base:** Convierte las aserciones de código Solidity a lógica de restricciones SMT, ejecutando Z3 de fondo.
*   **Vacío Exérgico:** **Impuesto de Máquina Virtual.** Aunque Z3 pruebe la corrección en compilación, el bytecode emitido se ejecuta sobre la EVM. La EVM sigue inyectando runtime guards para desbordamientos e introduce sobrecargas de gas y memoria. No existe transpilación a silicio directo (LLVM IR / eBPF / RISC-V).

---

## 4. Cristalización: Posicionamiento Estratégico de Anvil
El análisis crítico de los vectores SOTA expone el **vacío exérgico de la computación descentralizada actual**: los lenguajes que verifican la lógica de negocio (Solidity) compilan a entornos ineficientes (EVM), y los compiladores que generan código rápido y verificado a nivel físico (eBPF/RISC-V) carecen de semántica de negocio y tipos soberanos.

Anvil-Lang colapsa esta brecha estructural mediante la **Gobernanza de Invariantes**:

```
[Anvil Source] ──(where constraints)──> [Z3 Compile-Time Solver] (UNSAT Proof)
      │
      └─(Zero Runtime Guards)──────────> [LLVM IR Direct Codegen] ──> [Silicon (eBPF/RISC-V)]
```

Al unificar la semántica financiera soberana (`Wallet`, `Gas`, `TxHash`) con tipos de refinamiento e invariantes Hoare probados por Z3 y compilados directamente a LLVM IR sin comprobaciones redundantes en tiempo de ejecución, Anvil elimina tanto la sobrecarga cognitiva de anotación (vía sintaxis nativa `where`) como la sobrecarga física de la VM.
