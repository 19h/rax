from pathlib import Path
B=Path(__file__).resolve().parent;U=B.parent/'native-token-session-id-20261017';S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154');W=B/'windows'
def transform(text):
 return (text.replace(str(U),str(B)).replace('src-token-session-id','__NEW_SOURCE__').replace('src-token-open','src-token-session-id').replace('__NEW_SOURCE__','src-token-scalars')
 .replace('session-final','scalar-final').replace('session-owning','scalar-owning').replace('session-cpp','scalar-cpp').replace('session-root','scalar-root').replace('collect-session-archives','collect-scalar-archives').replace('verify-session-post-run','verify-scalar-post-run').replace('native-token-session-id-final','native-token-scalars-final'))
names=['native-session-final-core.ps1','run-session-final-core.py','run-session-final-observations.py','verify-session-final.py','assist-native-session-final-owned.ps1','run-session-final-owned.py','verify-session-owning-sources.py','verify-session-cpp-consumers.py','inspect-session-final-archives.py','verify-session-post-run.py','prepare-session-owning-junction.py']
for old in names:
 new=transform(old);text=transform((U/'windows'/old).read_text())
 if new=='run-scalar-final-core.py':text=text.replace('5713','5718')
 if new=='verify-scalar-owning-sources.py':text=text.replace('404 selected','409 selected')
 (W/new).write_text(text);(S/new).write_text(text)
(S/'collect-scalar-archives.py').write_text((B/'collect-scalar-archives.py').read_text())
print('Prepared unique native scalar final drivers; no launch performed')
