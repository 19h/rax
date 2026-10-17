from pathlib import Path
import hashlib,json,re
B=Path(__file__).resolve().parent;S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
a=json.loads((S/'native-image-extension-final-ordinary-outcomes.json').read_text());rows=[]
for i,row in enumerate(a):
 if 'log'not in row:rows.append(row);continue
 p=S/row['log'];raw=p.read_bytes();assert hashlib.sha256(raw).hexdigest()==row['sha256'];(B/p.name).write_bytes(raw);text=raw.decode(errors='replace');ends=re.findall(r'^terminal turn=(\d+) (.+)$',text,re.M);terminal=ends[-1]if ends else None
 result=dict(row,index=i,terminal=terminal,guest_exit=None)
 if terminal:
  match=re.fullmatch(r'Complete\(Exited\((\d+)\)\)',terminal[1]);result['guest_exit']=int(match.group(1))if match else None
 result['last-service']=(re.findall(r'^kernel-before turn=(\d+) service=(0x[0-9a-f]+) PC=(0x[0-9a-f]+)',text,re.M)or[None])[-1]
 result['old-terminal-turn']={0:2144480,2:2146842}.get(i)
 if terminal and result['old-terminal-turn']is not None:result['turn-delta']=int(terminal[0])-result['old-terminal-turn']
 rows.append(result)
(B/'ordinary-final-analysis.json').write_text(json.dumps(rows,indent=2)+'\n')
for row in rows:print(row.get('index'),row.get('terminal'),row.get('guest_exit'),row.get('last-service'))
