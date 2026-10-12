#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <winternl.h>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>
#include <intrin.h>
#if !defined(_M_ARM64)
#error This fresh producer captures verified ARM64 entry/return contexts only.
#endif
using Query=LONG(NTAPI *)(HANDLE,ULONG,PVOID,ULONG,PULONG);
struct EventInfo{ULONG type;LONG state;};
static uintptr_t pc(const CONTEXT& c){
#if defined(_M_ARM64)
 return c.Pc;
#elif defined(_M_X64)
 return c.Rip;
#else
 return c.Eip;
#endif
}
static void set_pc(CONTEXT& c,uintptr_t v){
#if defined(_M_ARM64)
 c.Pc=v;
#elif defined(_M_X64)
 c.Rip=v;
#else
 c.Eip=ULONG(v);
#endif
}
static uintptr_t stack(const CONTEXT& c){
#if defined(_M_ARM64)
 return c.Sp;
#elif defined(_M_X64)
 return c.Rsp;
#else
 return c.Esp;
#endif
}
static uintptr_t result(const CONTEXT& c){
#if defined(_M_ARM64)
 return c.X0;
#elif defined(_M_X64)
 return c.Rax;
#else
 return c.Eax;
#endif
}
static void fail(const char *what){printf("failure=%s error=%lu\n",what,GetLastError());ExitProcess(21);}
static void read(HANDLE process,uintptr_t at,void *out,SIZE_T size){SIZE_T got=0;if(!ReadProcessMemory(process,reinterpret_cast<void *>(at),out,size,&got)||got!=size)fail("read-child");}
static void write(HANDLE process,uintptr_t at,const void *input,SIZE_T size){SIZE_T put=0;if(!WriteProcessMemory(process,reinterpret_cast<void *>(at),input,size,&put)||put!=size)fail("write-child");}
static void code(HANDLE process,uintptr_t at,const std::vector<unsigned char>& bytes){DWORD old=0,ignored=0;if(!VirtualProtectEx(process,reinterpret_cast<void *>(at),bytes.size(),PAGE_EXECUTE_READWRITE,&old))fail("protect-child-code");write(process,at,bytes.data(),bytes.size());if(!VirtualProtectEx(process,reinterpret_cast<void *>(at),bytes.size(),old,&ignored)||!FlushInstructionCache(process,reinterpret_cast<void *>(at),bytes.size()))fail("restore-child-protection");}
static uintptr_t argument(HANDLE process,const CONTEXT& c){
 if(c.X0!=27||c.X2!=4||c.X4!=0)fail("original-trace-function-layout");
 ULONG value=0;read(process,c.X1,&value,4);return value;
}
static void set_argument(HANDLE process,CONTEXT& c,uintptr_t v){
 ULONG value=ULONG(v);write(process,c.X1,&value,4);
}
static uintptr_t caller(HANDLE,const CONTEXT& c){return c.Lr;}
static void repeat(HANDLE process,CONTEXT& c,uintptr_t entry,uintptr_t resume,uintptr_t arg){
 ULONG value=ULONG(arg);write(process,c.X1,&value,4);c.X0=27;c.X2=4;c.X3=0;c.X4=0;c.Lr=resume;set_pc(c,entry);
}

static std::vector<uintptr_t> handles(){
 std::vector<uintptr_t> result;for(uintptr_t value=4;value<65536;value+=4){DWORD flags=0;if(GetHandleInformation(reinterpret_cast<HANDLE>(value),&flags))result.push_back(value);}return result;
}
static void object_info(Query query,HANDLE event,unsigned char *buffer,const char *phase,unsigned ordinal){
 ULONG returned=0;LONG status=query(event,0,buffer,56,&returned);ULONG hc=0,pc=0;memcpy(&hc,buffer+8,4);memcpy(&pc,buffer+12,4);printf("object case=%u phase=%s status=%08lX returned=%lu handles=%lu pointers=%lu\n",ordinal,phase,ULONG(status),returned,hc,pc);if(status<0||returned!=56)fail("object-query");
}
int main(int argc,char **argv){
 if(argc>1){auto m=GetModuleHandleW(L"ntdll.dll");auto set=reinterpret_cast<LONG(NTAPI *)(ULONG,void *,ULONG,void *,ULONG,ULONG *)>(GetProcAddress(m,"NtTraceControl"));HANDLE event=CreateEventW(nullptr,FALSE,FALSE,nullptr);if(!set||!event)return 42;ULONG value=ULONG(uintptr_t(event)),returned=0;LONG status=set(27,&value,4,nullptr,0,&returned);CloseHandle(event);return status<0?43:41;}setvbuf(stdout,nullptr,_IONBF,0);
 auto module=GetModuleHandleW(L"ntdll.dll");auto entry=uintptr_t(GetProcAddress(module,"NtTraceControl"));auto dbg=GetProcAddress(module,"DbgBreakPoint");auto query=reinterpret_cast<Query>(GetProcAddress(module,"NtQueryEvent"));auto query_object=reinterpret_cast<Query>(GetProcAddress(module,"NtQueryObject"));if(!entry||!dbg||!query)fail("exports");
 SIZE_T instruction=sizeof(void *)==8?4:1;
#if !defined(_M_ARM64)
 instruction=1;
#endif
 std::vector<unsigned char> trap(instruction),original(instruction);memcpy(trap.data(),dbg,instruction);memcpy(original.data(),reinterpret_cast<void *>(entry),instruction);
#if defined(_M_ARM64)
 ULONG encoded=0;memcpy(&encoded,trap.data(),4);if((encoded&0xFFE0001F)!=0xD4200000)fail("native-break-instruction");
#else
 // x86/x64 INT3; ARM64X DbgBreakPoint exports may be hybrid thunks.
 trap[0]=0xCC;
#endif
 wchar_t executable[32768];if(!GetModuleFileNameW(nullptr,executable,32768))fail("executable-path");DWORD override_length=GetEnvironmentVariableW(L"ETW_CAPTURE_CHILD",executable,32768);if(override_length>=32768)fail("child-path-length");
 for(unsigned ordinal=0;ordinal<49;ordinal++){
  bool warmup=ordinal==0;unsigned index=warmup?0:ordinal-1,mode=warmup?12:index%12,initial=warmup?0:(index/12)%2,manual=warmup?0:index/24;
  auto before_handles=handles();
  DWORD hb=0,ha=0;if(!GetProcessHandleCount(GetCurrentProcess(),&hb))fail("handle-count-before");
  std::wstring command=L"\""+std::wstring(executable)+L"\" --child";STARTUPINFOW start{};start.cb=sizeof start;PROCESS_INFORMATION process{};
  if(!CreateProcessW(executable,command.data(),nullptr,nullptr,FALSE,DEBUG_ONLY_THIS_PROCESS,nullptr,nullptr,&start,&process))fail("debug-child");
  bool installed=false,entered=false,exited=false;unsigned calls=0;uintptr_t resume=0,real_arg=0,selected=0,scratch=0;std::vector<unsigned char> caller_bytes(instruction);HANDLE parent_event=nullptr,child_event=nullptr,replacement_parent=nullptr,replacement_child=nullptr;EventInfo before{},after{};ULONG rb=0,ra=0;LONG qb=LONG(0xDEADBEEF),qa=qb;ULONG first=0xDEADBEEF,second=first,third=first,fourth=first,exit_code=0;ULONG trace_returned[4]{};unsigned char object[56]{};ULONGLONG deadline=GetTickCount64()+30000;
  if(!warmup){
   parent_event=CreateEventW(nullptr,manual,initial,nullptr);if(!parent_event)fail("provided-parent-event");
   if(mode==0||(mode>=5&&mode<=10)){ULONG grant[]={0,1,2,3,0x100000,0x1F0003};ULONG rights=mode==0?0x1F0003:grant[mode-5];if(!DuplicateHandle(GetCurrentProcess(),parent_event,process.hProcess,&child_event,rights,FALSE,0))fail("provided-child-event");}
   else if(mode==11){HANDLE sem=CreateSemaphoreW(nullptr,0,1,nullptr);if(!sem||!DuplicateHandle(GetCurrentProcess(),sem,process.hProcess,&child_event,0,FALSE,DUPLICATE_SAME_ACCESS))fail("child-semaphore");CloseHandle(sem);}
  }
  replacement_parent=CreateEventW(nullptr,TRUE,FALSE,nullptr);if(!replacement_parent||!DuplicateHandle(GetCurrentProcess(),replacement_parent,process.hProcess,&replacement_child,0,FALSE,DUPLICATE_SAME_ACCESS))fail("replacement-child-event");
  while(!exited&&GetTickCount64()<deadline){
   DEBUG_EVENT event{};if(!WaitForDebugEvent(&event,1000)){if(GetLastError()==ERROR_SEM_TIMEOUT)continue;fail("debug-event");}
   DWORD disposition=DBG_CONTINUE;
   if((event.dwDebugEventCode==CREATE_PROCESS_DEBUG_EVENT||event.dwDebugEventCode==LOAD_DLL_DEBUG_EVENT)&&!installed){
    unsigned char bytes[32];SIZE_T got=0;MEMORY_BASIC_INFORMATION info{};
    if(VirtualQueryEx(process.hProcess,reinterpret_cast<void *>(entry),&info,sizeof info)&&info.Type==MEM_IMAGE&&ReadProcessMemory(process.hProcess,reinterpret_cast<void *>(entry),bytes,32,&got)&&got==32&&memcmp(bytes,reinterpret_cast<void *>(entry),32)==0){code(process.hProcess,entry,trap);installed=true;}
   }
   if(event.dwDebugEventCode==CREATE_PROCESS_DEBUG_EVENT){if(event.u.CreateProcessInfo.hFile)CloseHandle(event.u.CreateProcessInfo.hFile);}
   else if(event.dwDebugEventCode==LOAD_DLL_DEBUG_EVENT){if(event.u.LoadDll.hFile)CloseHandle(event.u.LoadDll.hFile);}
   else if(event.dwDebugEventCode==EXCEPTION_DEBUG_EVENT){
    auto address=uintptr_t(event.u.Exception.ExceptionRecord.ExceptionAddress);ULONG exception=event.u.Exception.ExceptionRecord.ExceptionCode;
    if((exception==EXCEPTION_BREAKPOINT||exception==0x4000001F||exception==EXCEPTION_ILLEGAL_INSTRUCTION)&&installed&&address==entry&&!entered){
     printf("trap case=%u phase=entry exception=%08lX address=%llX\n",ordinal,exception,(unsigned long long)address);unsigned char actual[4]{};read(process.hProcess,address,actual,instruction);if(memcmp(actual,trap.data(),instruction)!=0)fail("entry-trap-bytes");
     if(event.dwThreadId!=process.dwThreadId)fail("first-registration-thread");CONTEXT c{};c.ContextFlags=CONTEXT_CONTROL|CONTEXT_INTEGER;if(!GetThreadContext(process.hThread,&c))fail("entry-context");
     real_arg=argument(process.hProcess,c);selected=warmup?real_arg:mode==1?0:mode==2?uintptr_t(0x13572468):mode==3?uintptr_t(-1):mode==4?uintptr_t(-2):uintptr_t(child_event);resume=caller(process.hProcess,c);read(process.hProcess,resume,caller_bytes.data(),instruction);
     if(warmup&&!DuplicateHandle(process.hProcess,reinterpret_cast<HANDLE>(real_arg),GetCurrentProcess(),&parent_event,0,FALSE,DUPLICATE_SAME_ACCESS))fail("original-event-alias");
     scratch=uintptr_t(VirtualAllocEx(process.hProcess,nullptr,4096,MEM_RESERVE|MEM_COMMIT,PAGE_READWRITE));if(!scratch)fail("owned-scratch");c.X1=scratch;c.X5=scratch+8;ULONG returned_poison=0xA5A5A5A5;write(process.hProcess,c.X5,&returned_poison,4);
     qb=query(parent_event,0,&before,8,&rb);object_info(query_object,parent_event,object,"before",ordinal);code(process.hProcess,entry,original);code(process.hProcess,resume,trap);set_pc(c,entry);set_argument(process.hProcess,c,selected);if(!SetThreadContext(process.hThread,&c))fail("entry-resume-context");entered=true;
    }else if((exception==EXCEPTION_BREAKPOINT||exception==0x4000001F||exception==EXCEPTION_ILLEGAL_INSTRUCTION)&&entered&&address==resume){
     printf("trap case=%u phase=return-%u exception=%08lX address=%llX\n",ordinal,calls,exception,(unsigned long long)address);unsigned char actual[4]{};read(process.hProcess,address,actual,instruction);if(memcmp(actual,trap.data(),instruction)!=0)fail("return-trap-bytes");
     CONTEXT c{};c.ContextFlags=CONTEXT_CONTROL|CONTEXT_INTEGER;if(!GetThreadContext(process.hThread,&c))fail("return-context");
     read(process.hProcess,scratch+8,&trace_returned[calls],4);ULONG returned_poison=0xA5A5A5A5;write(process.hProcess,scratch+8,&returned_poison,4);c.X1=scratch;c.X5=scratch+8;
     if(calls==0){first=ULONG(result(c));printf("result case=%u call=0 status=%08lX\n",ordinal,first);calls++;repeat(process.hProcess,c,entry,resume,selected);if(!SetThreadContext(process.hThread,&c))fail("repeat-context");}
     else if(calls==1){second=ULONG(result(c));calls++;object_info(query_object,parent_event,object,"after-repeat",ordinal);
      if(warmup||mode==0||(mode>=5&&mode<=11)){HANDLE temporary=nullptr;if(!DuplicateHandle(process.hProcess,reinterpret_cast<HANDLE>(selected),GetCurrentProcess(),&temporary,0,FALSE,DUPLICATE_CLOSE_SOURCE))fail("close-child-selected");CloseHandle(temporary);}
      object_info(query_object,parent_event,object,"after-close",ordinal);repeat(process.hProcess,c,entry,resume,uintptr_t(replacement_child));if(!SetThreadContext(process.hThread,&c))fail("replacement-context");}
     else if(calls==2){third=ULONG(result(c));calls++;repeat(process.hProcess,c,entry,resume,uintptr_t(replacement_child));if(!SetThreadContext(process.hThread,&c))fail("replacement-repeat-context");}
     else{fourth=ULONG(result(c));calls++;qa=query(parent_event,0,&after,8,&ra);object_info(query_object,parent_event,object,"after-four",ordinal);code(process.hProcess,resume,caller_bytes);if(!TerminateProcess(process.hProcess,77))fail("terminate-owned-child");}
    }else if(exception!=EXCEPTION_BREAKPOINT&&exception!=0x4000001F){printf("unexpected-exception=%08lX address=%llX mode=%u\n",exception,(unsigned long long)address,mode);disposition=DBG_EXCEPTION_NOT_HANDLED;}
   }else if(event.dwDebugEventCode==EXIT_PROCESS_DEBUG_EVENT){exit_code=event.u.ExitProcess.dwExitCode;exited=true;}
   if(!ContinueDebugEvent(event.dwProcessId,event.dwThreadId,disposition))fail("continue-debug-event");
  }
  if(!exited){TerminateProcess(process.hProcess,78);fail("capture-deadline");}
  if(parent_event)CloseHandle(parent_event);if(replacement_parent)CloseHandle(replacement_parent);CloseHandle(process.hThread);CloseHandle(process.hProcess);if(!GetProcessHandleCount(GetCurrentProcess(),&ha))fail("handle-count-after");
  printf("case=%u warmup=%u width=%zu manual=%u initial=%u mode=%u child=%lu installed=%u entered=%u calls=%u entry=%llX caller=%llX original=%llX selected=%llX status=%08lX repeat=%08lX replacement=%08lX replacement-repeat=%08lX query-before=%08lX returned-before=%lu type-before=%lu state-before=%ld query-after=%08lX returned-after=%lu type-after=%lu state-after=%ld exit=%lu handles-before=%lu handles-after=%lu\n",ordinal,warmup,sizeof(void *),manual,initial,mode,process.dwProcessId,installed,entered,calls,(unsigned long long)entry,(unsigned long long)resume,(unsigned long long)real_arg,(unsigned long long)selected,first,second,third,fourth,ULONG(qb),rb,before.type,before.state,ULONG(qa),ra,after.type,after.state,exit_code,hb,ha);
  printf("trace-returned case=%u a=%08lX b=%08lX c=%08lX d=%08lX\n",ordinal,trace_returned[0],trace_returned[1],trace_returned[2],trace_returned[3]);
  if(!installed||!entered||calls!=4||exit_code!=77||qb<0||qa<0||(!warmup&&(hb!=ha||before_handles!=handles())))return 22;
 }
 puts("complete cases=48 warmup=1");return 0;
}
