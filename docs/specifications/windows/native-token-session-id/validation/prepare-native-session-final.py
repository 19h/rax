from pathlib import Path
B=Path(__file__).resolve().parent;S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
def transform(text):
 return (text.replace('src-token-open','__NEW_SOURCE__')
  .replace('token-final','session-final').replace('token-owning','session-owning')
  .replace('token-cpp','session-cpp').replace('token-root','session-root')
  .replace('collect-token-archives','collect-session-archives')
  .replace('__NEW_SOURCE__','src-token-session-id'))
pairs=[('native-token-final-core.ps1','native-session-final-core.ps1'),('run-token-final-core.py','run-session-final-core.py'),('run-token-final-observations.py','run-session-final-observations.py'),('verify-token-final.py','verify-session-final.py'),('assist-native-token-final-owned.ps1','assist-native-session-final-owned.ps1'),('run-token-final-owned.py','run-session-final-owned.py'),('verify-token-owning-sources.py','verify-session-owning-sources.py'),('verify-token-cpp-consumers.py','verify-session-cpp-consumers.py'),('inspect-token-final-archives.py','inspect-session-final-archives.py'),('verify-token-post-run.py','verify-session-post-run.py')]
for old,new in pairs:
 text=transform((S/old).read_text())
 if new=='run-session-final-core.py':text=text.replace('5677','5713')
 if new=='verify-session-owning-sources.py':text=text.replace('401 selected','404 selected')
 (S/new).write_text(text);(B/'windows'/new).write_text(text)
text=(S/'prepare-token-owning-junction.py').read_text().replace('src-token-open','__NEW__').replace('src-dynamic-table','src-token-open').replace('__NEW__','src-token-session-id').replace('token-owning','session-owning')
(S/'prepare-session-owning-junction.py').write_text(text);(B/'windows/prepare-session-owning-junction.py').write_text(text)
(S/'collect-session-archives.py').write_text((B/'collect-session-archives.py').read_text())
print('Prepared unique native session final drivers; not launched')
