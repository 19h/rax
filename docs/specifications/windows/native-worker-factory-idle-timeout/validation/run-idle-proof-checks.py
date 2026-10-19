from pathlib import Path
import subprocess
import sys

source = Path(sys.argv[1]).resolve()
evidence = source / 'docs/specifications/windows/native-worker-factory-idle-timeout'
for argv in [[sys.executable, '-B', str(evidence / 'check_native.py'), str(evidence)],
             [sys.executable, '-B', str(evidence / 'check_validation.py'), str(source)]]:
    subprocess.run(argv, cwd=source, check=True)
