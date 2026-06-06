import re
import os

with open('src/engine/verifier.rs', 'r') as f:
    lines = f.readlines()

def extract(start_pat, end_pat=None):
    start_idx = -1
    for i, line in enumerate(lines):
        if re.search(start_pat, line):
            start_idx = i
            break
    if start_idx == -1:
        return []
    
    if end_pat is None:
        return lines[start_idx:]
    
    end_idx = -1
    for i in range(start_idx+1, len(lines)):
        if re.search(end_pat, lines[i]):
            end_idx = i
            break
            
    if end_idx == -1:
        return lines[start_idx:]
    return lines[start_idx:end_idx]

os.makedirs('src/engine/verifier', exist_ok=True)
