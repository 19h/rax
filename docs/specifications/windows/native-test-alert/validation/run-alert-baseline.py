from pathlib import Path
import datetime,hashlib,json,os,re,subprocess
P=Path(__file__).parent;R=Path('/Users/int/.assist-alert-20261019');record=P/'alert-baseline-a.json';assert not record.exists();paths=['src/user/windows/process/sched/tests/services_tests.rs','src/user/windows/process/sched/tests/services_tests/test_alert_tests.rs']
def inputs():return [{'path':x,'bytes':(R/x).stat().st_size,'sha256':hashlib.sha256((R/x).read_bytes()).hexdigest()}for x in paths]
before=inputs();argv=['cargo','+1.95.0','test','--locked','--offline','--no-default-features','--lib','native_test_alert','--','--nocapture','--test-threads=1'];log=P/'alert-baseline-a.log';(R/'t').mkdir(exist_ok=True)
with log.open('wb')as out:code=subprocess.run(argv,cwd=R,env=dict(os.environ,CARGO_BUILD_JOBS='2',CARGO_INCREMENTAL='0',TMPDIR=str(R/'t')),stdout=out,stderr=subprocess.STDOUT).returncode
assert before==inputs();raw=log.read_bytes();text=raw.decode('utf8','replace');d={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'exit-code':code,'argv':argv,'log':log.name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest(),'inputs':before,'results':re.findall(r'test result: ([^\r\n]+)',text)};record.write_text(json.dumps(d,indent=2)+'\n');assert code==101 and '0 passed; 2 failed;'in text and 'error[E'not in text and 'Finished `test` profile'in text
print('Compiled baseline failed exactly two unsupported-service tests',flush=True)
