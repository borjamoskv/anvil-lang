import os
import random

MAX_U64 = 0xFFFFFFFFFFFFFFFF

# ====================================================================
# [C5-REAL] CORTEX-Persist: Autonomous Bounty Scanner Engine
# ====================================================================
# Este pipeline asimila múltiples modelos de contratos, los reduce a su
# esqueleto termodinámico y ejecuta fuzzing en los límites de la máquina.
# ====================================================================

class AMMPoolTarget:
    name = "AMM Constant Product (Vela/Uniswap Fork)"
    bounty = "$500,000"
    
    @staticmethod
    def execute_swap(reserve_x, reserve_y, amount_in):
        amount_in_with_fee = (amount_in * 99) & MAX_U64
        numerator = (amount_in_with_fee * reserve_y) & MAX_U64
        term1 = (reserve_x * 100) & MAX_U64
        denominator = (term1 + amount_in_with_fee) & MAX_U64
        if denominator == 0: return None
        amount_out = numerator // denominator
        return (reserve_x + amount_in) & MAX_U64, (reserve_y - amount_out) & MAX_U64

class LendingPoolTarget:
    name = "Lending Protocol Close Factor (K2/Aave Fork)"
    bounty = "$1,000,000"
    
    @staticmethod
    def execute_liquidation(collateral, debt, liquidation_amount):
        # Vulnerabilidad real (K2 Bypass):
        # El contrato verifica que amount * 100 <= debt * 50 (50% max close factor)
        left_side = (liquidation_amount * 100) & MAX_U64
        right_side = (debt * 50) & MAX_U64
        
        # Validación defectuosa
        if left_side > right_side:
            return None # Reverted
            
        remaining_debt = debt - liquidation_amount
        remaining_collateral = collateral - liquidation_amount 
        return remaining_debt, remaining_collateral

TARGET_MODELS = [AMMPoolTarget, LendingPoolTarget]

def scan_amm(target):
    found = False
    for _ in range(5000):
        delta = random.randint(1, 100)
        rx = (MAX_U64 // 100) + delta
        ry = random.randint(1_000_000, 10_000_000)
        a_in = random.randint(10_000, 50_000)
        res = target.execute_swap(rx, ry, a_in)
        if not res: continue
        rx_prime, ry_prime = res
        
        # Ley Inquebrantable
        if rx_prime * ry_prime < rx * ry:
            print(f"    [!] EXPLOIT ENCONTRADO (Invariante Termodinámico roto):")
            print(f"        Valores C5-REAL: reserve_x={rx}, reserve_y={ry}, amount_in={a_in}")
            found = True
            break
    return found

def scan_lending(target):
    found = False
    for _ in range(5000):
        # Para engañar al chequeo (amount * 100 <= debt * 50):
        # debt debe ser tal que (debt * 100) desborda y se vuelve un número minúsculo,
        # mientras que (debt * 50) NO desborda (es grande).
        # Esto ocurre si MAX_U64/100 < debt < MAX_U64/50.
        min_debt = (MAX_U64 // 100) + 1
        max_debt = (MAX_U64 // 50) - 1
        
        debt = random.randint(min_debt, max_debt)
        collateral = debt * 2
        
        # Atacante intenta liquidar el 100%
        liquidation_amount = debt 
        
        res = target.execute_liquidation(collateral, debt, liquidation_amount)
        if not res: continue
        
        remaining_debt, remaining_collateral = res
        
        # Invariante: Nunca permitir liquidar más del 50% real de la deuda total.
        # Si remaining_debt == 0, se liquidó el 100%.
        if remaining_debt == 0 and debt > 0:
            print(f"    [!] EXPLOIT ENCONTRADO (Close Factor Bypass detectado):")
            print(f"        Liquidación del 100% permitida por overflow en límite.")
            print(f"        Valores C5-REAL: debt={debt}, liquidation_amount={liquidation_amount}")
            found = True
            break
    return found

def run_scanner():
    print("==========================================================")
    print("🛡️  CORTEX-Persist: Unified Bounty Scanner (C5-REAL)")
    print("==========================================================")
    
    for target in TARGET_MODELS:
        print(f"[*] Evaluando Objetivo: {target.name} [Bounty: {target.bounty}]")
        print("    Buscando asimetrías matemáticas...")
        
        exploited = False
        if "AMM" in target.name:
            exploited = scan_amm(target)
        elif "Lending" in target.name:
            exploited = scan_lending(target)
            
        if not exploited:
            print("    [*] Seguro. No se violaron las leyes de conservación.")
        print("-" * 58)

if __name__ == "__main__":
    run_scanner()
