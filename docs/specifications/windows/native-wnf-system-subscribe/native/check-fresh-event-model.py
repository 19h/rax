from pathlib import Path
import json
b=Path(__file__).resolve().parent
system=json.loads((b/'native-system-wnf-subscribe-fresh-verified.json').read_text())
owned=json.loads((b/'native-system-wnf-subscribe-owned-fresh-verified.json').read_text())
profiles=[('captured-system',1,1,system['rows'])]
profiles += [(r['mode'],3,0 if r['mode']=='empty' else 1,r['rows']) for r in owned['runs']]
checked=[]
for label,grant,stamp,rows in profiles:
 for row in rows:
  if row['warmup']=='1':continue
  mask=int(row['mask'],16);requested=int(row['stamp'],16);reg_first=row['after']=='1'
  required=(1 if mask&0x11 else 0)|(2 if mask&0x0e else 0)
  allowed=grant&required==required
  levels=8|(2 if mask&1 else 4 if mask&0x0e else 0)
  initial=(mask&levels)|(1 if mask&1 and stamp!=0 and requested!=stamp else 0)
  second=1 if mask&1 and stamp!=0 and 0xffffffff!=stamp and initial==0 else 0
  expected=[0,0,0,0,0]
  if allowed:
   expected[2]=int(reg_first and initial!=0)
   expected[3]=int(expected[2]!=0 or second!=0)
   expected[4]=expected[3]
  actual=[int(row['state'+str(i)])for i in range(5)]
  if actual!=expected:raise ValueError((label,row,expected))
 checked.append({'profile':label,'rows':len(rows)-1,'all-matched':True})
(b/'fresh-event-model-check.json').write_text(json.dumps({'formula':'L(m)=8|(m&1?2:m&14?4:0); initial=m&L(m) plus new-data bit for nonzero selected stamp unequal to requested; a nonempty notification queue does not resignal when data is added; registration does not retroactively signal','profiles':checked,'tested-cases':sum(x['rows']for x in checked)},indent=2)+'\n')
print(checked)
