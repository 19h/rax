from pathlib import Path
p=Path('C:/Windows/Temp/assist-native-rax-20261010/native-wnf-query-final4-full.log')
print('bytes',p.stat().st_size)
with p.open('rb') as f:
 f.seek(max(0,p.stat().st_size-3000));print(f.read().decode('utf-8',errors='replace')[-1900:])
