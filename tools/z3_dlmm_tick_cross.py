import time
from z3 import *

print("==========================================================")
print("🛡️  CORTEX-Persist: Z3 DLMM Tick Crossing (C5-REAL)")
print("==========================================================")

s = Solver()

# Constantes (Escala de precisión para el precio en DLMMs suele ser 10^8 o 10^6)
PRICE_SCALE = 1000000

# Estado Inicial del Pool (2 Bins contiguos)
# Bin 1 (Activo)
bin1_price = Int('bin1_price')
bin1_x = Int('bin1_x')
bin1_y_scaled = Int('bin1_y_scaled')

# Bin 2 (Siguiente)
bin2_price = Int('bin2_price')
bin2_x = Int('bin2_x')
bin2_y_scaled = Int('bin2_y_scaled')

# Atacante: Swap X -> Y que cruza el límite
swap_in_x = Int('swap_in_x')

# Precondiciones y configuración del escenario
s.add(bin1_price > 0)
s.add(bin2_price > bin1_price)  # Precio estricto monótono
s.add(bin1_x >= 0, bin1_y_scaled > 0)
s.add(bin2_x >= 0, bin2_y_scaled > 0)

# Atacante inyecta cantidad precisa que agota Bin 1 y entra en Bin 2
s.add(swap_in_x > 0)

# Lógica de Swap X por Y (Tick Crossing)
# Paso 1: Agotar Bin 1
# y_out_1 = (amount_in_1 * bin1_price) / PRICE_SCALE
amount_in_1 = Int('amount_in_1')
y_out_1 = Int('y_out_1')
s.add(y_out_1 == bin1_y_scaled) # Vaciamos todo el Y escalado del Bin 1
# amount_in_1 es lo que cuesta vaciar el Bin 1 (con floor division a favor del protocolo)
s.add(amount_in_1 == (y_out_1 * PRICE_SCALE) / bin1_price) 

# Paso 2: El remanente pasa al Bin 2
amount_in_2 = swap_in_x - amount_in_1
y_out_2 = Int('y_out_2')
s.add(amount_in_2 > 0) # Aseguramos que cruza
s.add(y_out_2 == (amount_in_2 * bin2_price) / PRICE_SCALE)

# Salida total del atacante
total_y_out_scaled = y_out_1 + y_out_2

# Paso 3: Atacante hace Swap inverso (Y -> X) para intentar el Arbitraje
# Vuelve a cruzar los bins en dirección opuesta
reverse_swap_in_y = total_y_out_scaled
amount_in_rev_2 = y_out_2 # Devolvemos exactamente lo que sacamos del Bin 2
x_out_2 = Int('x_out_2')
s.add(x_out_2 == (amount_in_rev_2 * PRICE_SCALE) / bin2_price)

amount_in_rev_1 = reverse_swap_in_y - amount_in_rev_2
x_out_1 = Int('x_out_1')
s.add(x_out_1 == (amount_in_rev_1 * PRICE_SCALE) / bin1_price)

total_x_out = x_out_1 + x_out_2

# Condición de Vulnerabilidad (Arbitraje Termodinámico / Money Printer)
# ¿El atacante sale con MÁS X del que metió inicialmente, a pesar de cruzar Bins exactos?
s.add(total_x_out > swap_in_x)

print("[*] Buscando asimetría de redondeo en el cruce de Bins (Tick Crossing)...")
start = time.time()
res = s.check()
elapsed = time.time() - start

if res == sat:
    print(f"[!] SATISFIABLE: ¡Money Printer encontrado en {elapsed:.2f}s!")
    m = s.model()
    print("\n--- Parámetros del Exploit ---")
    print(f"Swap Inicial X: {m[swap_in_x].as_long()}")
    print(f"Bin1 Price:     {m[bin1_price].as_long()} | Bin1 Y: {m[bin1_y_scaled].as_long()}")
    print(f"Bin2 Price:     {m[bin2_price].as_long()} | Bin2 Y: {m[bin2_y_scaled].as_long()}")
    print("\n--- Resultado del Arbitraje ---")
    print(f"Total Y obtenido: {m[total_y_out_scaled].as_long()}")
    print(f"Total X recuperado (Swap inverso): {m[total_x_out].as_long()}")
    print(f"Beneficio Neto X: {m[total_x_out].as_long() - m[swap_in_x].as_long()}")
else:
    print(f"[*] UNSAT: El redondeo en el Tick Crossing es conservador. Tiempo: {elapsed:.2f}s")
