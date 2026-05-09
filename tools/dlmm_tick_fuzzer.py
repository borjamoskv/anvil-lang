import random
import time

print("==========================================================")
print("🐍 [OUROBOROS] DLMM Tick Crossing Fuzzer (C5-REAL)")
print("==========================================================")

PRICE_SCALE = 1000000


def simulate_tick_cross(bin1_price, bin2_price, bin1_y, swap_in_x):
    # Paso 1: Agotar Bin 1 (X -> Y)
    amount_in_1_floor = (bin1_y * PRICE_SCALE) // bin1_price

    if amount_in_1_floor >= swap_in_x:
        return 0

    rem_x = swap_in_x - amount_in_1_floor
    y_out_2 = (rem_x * bin2_price) // PRICE_SCALE

    # Paso 2: Swap Inverso (Y -> X)
    # Devolvemos y_out_2 al Bin 2
    rev_x_2 = (y_out_2 * PRICE_SCALE) // bin2_price

    # Devolvemos bin1_y al Bin 1
    rev_x_1 = (bin1_y * PRICE_SCALE) // bin1_price

    total_x_out = rev_x_1 + rev_x_2

    return total_x_out - swap_in_x


def run_fuzzer(iterations=1000000):
    print(f"[*] Fuzzing {iterations} tick crossing scenarios...")
    start_time = time.time()
    found = 0

    for i in range(iterations):
        bin1_price = random.randint(100, 100000)
        bin2_price = bin1_price + random.randint(1, 1000)
        bin1_y = random.randint(10, 10000)

        # swap_in_x debe ser suficiente para cruzar
        min_x = (bin1_y * PRICE_SCALE) // bin1_price
        swap_in_x = min_x + random.randint(1, 1000)

        profit = simulate_tick_cross(bin1_price, bin2_price, bin1_y, swap_in_x)

        if profit > 0:
            found += 1
            if found <= 5:
                print(f"\n[!] MONEY PRINTER DETECTED (Profit X: {profit})")
                print(f"    Bin1 Price: {bin1_price} | Bin2 Price: {bin2_price}")
                print(f"    Swap In X: {swap_in_x} | Total X Out: {swap_in_x + profit}")

    end_time = time.time()
    print(f"\n[*] Fuzzing complete in {end_time - start_time:.2f}s")
    print(f"[*] Total exploits found: {found}")


if __name__ == "__main__":
    run_fuzzer()
