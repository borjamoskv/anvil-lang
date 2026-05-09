# C5-REAL Audit Report: AMM Constant Product Invariant Bypass
**Target:** AMM Pool (Uniswap V2 Clone)  
**Bounty Platform:** Immunefi ($500,000 Critical Tier)  
**Vector:** Mathematical precision loss leading to invariant collapse ($x \cdot y = k$).

## 1. Thermodynamic Context (The Bug)
El motor Anvil (con Z3 Solver) fue inyectado con el código fuente del AMM. El contrato intentaba mantener el invariante de liquidez de producto constante (`reserve_x' * reserve_y' >= reserve_x * reserve_y`) tras aplicar una tarifa del 1%.

Sin embargo, el cálculo del denominador en la función de *swap* es vulnerable:
```rust
let denominator = (reserve_x * 100) + amount_in_with_fee;
```
Z3 detectó que para un `reserve_x` masivo (alta liquidez en el pool), la multiplicación `reserve_x * 100` sufre un **Integer Overflow** silente en registros de 64 bits.

## 2. SAT Model (El Exploit Geométrico)
Z3 no encontró un "posible" error. Z3 falsó la ecuación y proveyó las coordenadas exactas de colapso:
- `reserve_x` = `184,467,440,737,095,500` (Liquidez del protocolo)
- `reserve_y` = `1,000,000` (Token a drenar)
- `amount_in` = `1,000` (Flashloan mínimo)

**Efecto Cascada:**
1. El atacante deposita `amount_in` (1,000).
2. El denominador se calcula como `reserve_x * 100`. Esto excede el máximo de `u64` (18.4 trillones) y envuelve (wrap-around) a un número minúsculo (ej: `5`).
3. El `amount_out` se calcula dividiendo el numerador por este denominador minúsculo (`5`), lo que resulta en un retiro colosal que excede la liquidez matemática prevista.
4. El atacante drena todo el `reserve_y` con un input mínimo.

## 3. Resolución Epistémica
El invariante matemático fue roto.
La vulnerabilidad permite drenar el 100% del TVL (Total Value Locked).

**Severidad:** CRITICAL (C5-REAL).
**Fricción de Extracción:** Cero. El path de ataque fue generado automáticamente por la falsación del solver SMT.

## 4. Remediation (El Parche de Silicio)
Para arreglar este agujero termodinámico, el protocolo debe utilizar matemáticas seguras contra el desbordamiento o elevar la precisión del registro a 128/256 bits, y lo más importante: **El chequeo del invariante K no puede estar implícito en el cálculo del swap, debe ser afirmado de forma explícita al final de la ejecución**.

```rust
assert!(reserve_x' * reserve_y' >= reserve_x * reserve_y, "K-Invariant violated");
```

---
*Auditado por CORTEX-Persist (Anvil-Lang v0.5). Where trust doesn't compile.*
