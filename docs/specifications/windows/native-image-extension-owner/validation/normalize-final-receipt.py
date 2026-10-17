from pathlib import Path
import hashlib,json,sys
B=Path(__file__).resolve().parent;host=sys.argv[1]
raw=B/(host+'-wnf-system-epoch2-gates.json');data=raw.read_bytes();rows=json.loads(data)
assert [r['gate']for r in rows]==['focus','full','capi','integration','all-targets']
for r in rows:
 assert r['exit-code']==0 and r['log']==host+'-image-extension-final-'+r['gate']+'.log'
 assert hashlib.sha256((B/r['log']).read_bytes()).hexdigest()==r['sha256']
result={'host':host,'source-epoch':'epoch4 shared inputs; epochs5 and6 only change cfg(windows) tests','actual-source':str(B/'rax-source-final'),'original-receipt-name':raw.name,'original-receipt-sha256':hashlib.sha256(data).hexdigest(),'note':'Executed driver retained a historical receipt filename. Exact final rows are identified by unique log paths, commands and raw log digests; no test results were changed.','gates':rows}
(B/(host+'-image-extension-final-gates.json')).write_text(json.dumps(result,indent=2)+'\n')
(B/(host+'-image-extension-final-original-receipt.json')).write_bytes(data)
print(host,'verified final receipt',len(rows),'gates')
