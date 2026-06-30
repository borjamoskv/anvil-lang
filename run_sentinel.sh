#!/usr/bin/env bash
# C5-REAL Sovereign Executor para Inbound Sentinel
echo "[0x00_INIT] Inicializando Cuarentena de Ejecución Física (Vector 2)"
echo "[0x00_INIT] Aplicando perfil restrictivo sentinel.sb..."

sandbox-exec -f sentinel.sb python3 inbound_sentinel.py
