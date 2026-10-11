from pathlib import Path
import hashlib,json,sys
def require(ok, message):
 if not ok:raise ValueError(message)
require(not sys.flags.optimize, 'optimized validation is rejected')
root=Path(r'C:\Windows\Temp\assist-native-rax-20261010')
artifacts=[json.loads(x) for x in (root/'native-io-completion-after-artifacts.jsonl').read_text().splitlines()]
core=[x for x in artifacts if x.get('reason')=='compiler-artifact' and x['target']['name']=='rax' and not x['manifest_path'].replace('\\','/').endswith('/capi/Cargo.toml')]
require(len(core)==1 and core[0]['features']==[], 'selected empty-feature core')
paths=[root/'target-io-completion-owned/debug/assist_rs.lib',Path(next(p for p in core[0]['filenames'] if p.endswith('.rlib')))]
rows=[]
for path in paths:
 size=path.stat().st_size;h=hashlib.sha256()
 with path.open('rb') as f:
  for block in iter(lambda:f.read(1048576),b''):h.update(block)
 with path.open('rb') as f:
  require(f.read(8)==b'!<arch>\n', 'archive magic');count=0
  while f.tell()<size:
   header=f.read(60);require(len(header)==60 and header[58:60]==b'`\n', 'member header')
   member=int(header[48:58]);require(member>=0, 'nonnegative member extent');end=f.tell()+member+(member&1);require(end<=size, 'complete member extent')
   f.seek(end);count+=1
  require(f.tell()==size, 'complete archive walk')
 rows.append({'path':str(path),'bytes':size,'sha256':h.hexdigest(),'members':count,'structural-member-walk':'pass'})
sources=[]
for x in [{'path': 'src/user/windows/dll/native.rs', 'bytes': 14082, 'sha256': '70711e96807db0a2da5fc480d2bfeaa0c0beec9dc12652f6d3942fd9b66f9774'}, {'path': 'src/user/windows/dll/native/events.rs', 'bytes': 3767, 'sha256': 'bebbd7af35387bbf73d6767a726f6f771f8a29c628f7d7fd40f8a9565715acdf'}, {'path': 'src/user/windows/dll/native/io_completion.rs', 'bytes': 2310, 'sha256': 'b5b103510a717402d8b76f7bd3aeae42fe12cb2b05d4338493b68e25ce438df5'}, {'path': 'src/user/windows/dll/native/object_create.rs', 'bytes': 4585, 'sha256': '789a8d82dfc517b9b8075446c8f2ea8eca15d3cbeb9a1fcd2c05c06f6980ec14'}, {'path': 'src/user/windows/loader/ldr.rs', 'bytes': 15508, 'sha256': 'e53c185d04e45bfa8e0b27cdfce287470cb13630c224733df40f32cd0248f206'}, {'path': 'src/user/windows/native.rs', 'bytes': 15319, 'sha256': '196fc678e72a648277975ca17d25a9a0b5a4c327a1df4d08761bf669bb0fc5a6'}, {'path': 'src/user/windows/objects.rs', 'bytes': 18192, 'sha256': '85f8f844535619de55523fc38a4961e283599f07588c32d5bc368bb6632d3714'}, {'path': 'src/user/windows/process/mod.rs', 'bytes': 15393, 'sha256': '32b80155df3fe6e09a17f14ac8fa3f4c72753692746990811ec7d5500a73e259'}, {'path': 'src/user/windows/process/native_start.rs', 'bytes': 5368, 'sha256': '5be69f45d98c32f1e969fd44391008b3773caecfb849fcf675b2cebd7e5312aa'}, {'path': 'src/user/windows/process/native_start/tests.rs', 'bytes': 12036, 'sha256': 'd4528966026393c59be195167b7e80632b0f7a8379f87af9a9704f0d32ecca42'}, {'path': 'src/user/windows/process/sched.rs', 'bytes': 60944, 'sha256': '7d293a4365d725166eea0b89698f409ed0483f0c7d62763a31bf641a694201c4'}, {'path': 'src/user/windows/process/sched/tests/services_tests.rs', 'bytes': 45507, 'sha256': '1b54b5d5464708f7dff60d3bef25690738b09035c33bb66630c99e25d037b963'}, {'path': 'src/user/windows/process/sched/tests/services_tests/io_completion_create_leaf_tests.rs', 'bytes': 5463, 'sha256': '786f85845c758e65f3eae30fcc0a999af94272429a2071cc15ef407a822da530'}, {'path': 'src/user/windows/process/sched/tests/services_tests/io_completion_create_replay_tests.rs', 'bytes': 10495, 'sha256': 'd992ab4b827fc0dd54248a4905c996850133a9bf99f960d3b5ed6553d87ade67'}, {'path': 'src/user/windows/process/sched/tests/services_tests/io_completion_create_tests.rs', 'bytes': 7738, 'sha256': '4a1ce2de779d24b572c16f772dd48a438e34d5e61c64b766be3b98ab7e144da8'}, {'path': 'src/user/windows/process/start.rs', 'bytes': 21160, 'sha256': '594da70046fbed03123703d25414b7b5f1cb017f9c188414e2b16679f36aebb1'}, {'path': 'src/user/windows/process/thread.rs', 'bytes': 18198, 'sha256': '37c72833cabe4e082218884635b52f4598a16f3ee592a1a471c9eaa3fdff4ea2'}, {'path': 'src/user/windows/sync.rs', 'bytes': 35252, 'sha256': 'b40b645148f512e5fa2913710ffa71823394af6ed62f4da803055d4ac5c7f601'}]:
 data=(root/'src'/x['path']).read_bytes();require(len(data)==x['bytes'] and hashlib.sha256(data).hexdigest()==x['sha256'], 'native source '+x['path']);sources.append(x)

helpers = [{'path': 'io-completion-owned-cpp-build.py', 'bytes': 1839, 'sha256': '9995e244e446791e41fc1d813aaf59db7ccb7b472c064785232f4e11f63d3608', 'native-path': 'C:\\Windows\\Temp\\assist-native-rax-20261010\\io-completion-owned-cpp-build.py'}, {'path': 'native-process-probe.cpp', 'bytes': 2187, 'sha256': 'f57571cc5961784ff841b84811bdff7626ccd582dda3a16a9be35c9aeedc5a31', 'native-path': 'C:\\Windows\\Temp\\assist-native-rax-20261010\\assist\\native-process-probe.cpp'}]
for item in helpers:
 data=Path(item['native-path']).read_bytes();require(len(data)==item['bytes'] and hashlib.sha256(data).hexdigest()==item['sha256'], 'owning actual helper '+item['path'])
print(json.dumps({'archives':rows,'core-features':core[0]['features'],'sources':sources,'helpers':helpers}))
