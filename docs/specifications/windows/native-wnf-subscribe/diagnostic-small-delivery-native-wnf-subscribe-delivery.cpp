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
#if defined(_M_ARM64)
 return c.X0;
#elif defined(_M_X64)
 return c.Rcx;
#else
 ULONG v=0;read(process,c.Esp+4,&v,4);return v;
#endif
}
static void set_argument(HANDLE process,CONTEXT& c,uintptr_t v){
#if defined(_M_ARM64)
 c.X0=v;
#elif defined(_M_X64)
 c.Rcx=v;
#else
 ULONG value=ULONG(v);write(process,c.Esp+4,&value,4);
#endif
}
static uintptr_t caller(HANDLE process,const CONTEXT& c){
#if defined(_M_ARM64)
 return c.Lr;
#else
 uintptr_t v=0;read(process,stack(c),&v,sizeof v);return v;
#endif
}
static void repeat(HANDLE process,CONTEXT& c,uintptr_t entry,uintptr_t resume,uintptr_t arg){
#if defined(_M_ARM64)
 c.Lr=resume;c.X0=arg;
#elif defined(_M_X64)
 c.Rsp-=8;write(process,c.Rsp,&resume,8);c.Rcx=arg;
#else
 c.Esp-=8;ULONG ret=ULONG(resume),value=ULONG(arg);write(process,c.Esp,&ret,4);write(process,c.Esp+4,&value,4);
#endif
 set_pc(c,entry);
}

static std::vector<uintptr_t> handles(){
 std::vector<uintptr_t> result;for(uintptr_t value=4;value<65536;value+=4){DWORD flags=0;if(GetHandleInformation(reinterpret_cast<HANDLE>(value),&flags))result.push_back(value);}return result;
}
static void object_info(Query query,HANDLE event,unsigned char *buffer,const char *phase,unsigned ordinal){
 ULONG returned=0;LONG status=query(event,0,buffer,56,&returned);ULONG hc=0,pc=0;memcpy(&hc,buffer+8,4);memcpy(&pc,buffer+12,4);printf("object case=%u phase=%s status=%08lX returned=%lu handles=%lu pointers=%lu\n",ordinal,phase,ULONG(status),returned,hc,pc);if(status<0||returned!=56)fail("object-query");
}
#if !defined(_M_ARM64)
#error This producer measures native ARM64 process startup only.
#endif
static void invoke(CONTEXT& c,uintptr_t entry,uintptr_t resume,uint64_t a0,uint64_t a1=0,uint64_t a2=0,uint64_t a3=0,uint64_t a4=0,uint64_t a5=0){c.Pc=entry;c.Lr=resume;c.X0=a0;c.X1=a1;c.X2=a2;c.X3=a3;c.X4=a4;c.X5=a5;}
int main(int argc,char **){
 auto m=GetModuleHandleW(L"ntdll.dll");auto setter=uintptr_t(GetProcAddress(m,"NtSetWnfProcessNotificationEvent"));auto subscribe=uintptr_t(GetProcAddress(m,"NtSubscribeWnfStateChange"));auto unsubscribe=uintptr_t(GetProcAddress(m,"NtUnsubscribeWnfStateChange"));
 auto complete=uintptr_t(GetProcAddress(m,"NtGetCompleteWnfStateSubscription"));if(!complete)fail("complete-export");
 if(argc>1){HANDLE event=CreateEventW(nullptr,FALSE,FALSE,nullptr);if(!event)return 42;reinterpret_cast<LONG(NTAPI *)(HANDLE)>(setter)(event);CloseHandle(event);return 41;}
 setvbuf(stdout,nullptr,_IONBF,0);auto query=reinterpret_cast<Query>(GetProcAddress(m,"NtQueryEvent"));auto dbg=GetProcAddress(m,"DbgBreakPoint");if(!setter||!subscribe||!unsubscribe||!query||!dbg)fail("exports");
 std::vector<unsigned char> trap(4),original(4);memcpy(trap.data(),dbg,4);memcpy(original.data(),reinterpret_cast<void *>(setter),4);ULONG encoded=0;memcpy(&encoded,trap.data(),4);if((encoded&0xFFE0001F)!=0xD4200000)fail("native-break");
 wchar_t exe[32768];if(!GetModuleFileNameW(nullptr,exe,32768))fail("path");
 for(unsigned ordinal=0;ordinal<129;ordinal++){
  bool warmup=ordinal==0;unsigned index=warmup?0:ordinal-1,after=index/64;ULONG masks[]={0,1,2,4,8,0x10,0x11,0x1F};ULONG mask=masks[(index%64)/8],next_mask=masks[index%8];DWORD hb=0,ha=0;GetProcessHandleCount(GetCurrentProcess(),&hb);auto before_handles=handles();
  std::wstring command=L"\""+std::wstring(exe)+L"\" --child";STARTUPINFOW start{};start.cb=sizeof start;PROCESS_INFORMATION p{};if(!CreateProcessW(exe,command.data(),nullptr,nullptr,FALSE,DEBUG_ONLY_THIS_PROCESS,nullptr,nullptr,&start,&p))fail("create-child");
  bool installed=false,entered=false,exited=false;unsigned phase=0,calls=0;uintptr_t resume=0,event_arg=0,remote=0;std::vector<unsigned char> saved(4);HANDLE event=nullptr;ULONG set_status=0xDEADBEEF,sub_status=set_status,repeat_status=set_status,unsub_status=set_status,exit=0;uint64_t id=0,id2=0;ULONG get_status[2]={0xDEADBEEF,0xDEADBEEF};unsigned char descriptors[2][48]{};LONG states[7]={-1,-1,-1,-1,-1,-1,-1};ULONG types[7]={~0u,~0u,~0u,~0u,~0u,~0u,~0u};ULONGLONG deadline=GetTickCount64()+30000;
  auto observe=[&](unsigned n){EventInfo i{};ULONG size=0;LONG status=query(event,0,&i,8,&size);if(status<0||size!=8)fail("event-query");states[n]=i.state;types[n]=i.type;};
  while(!exited&&GetTickCount64()<deadline){DEBUG_EVENT e{};if(!WaitForDebugEvent(&e,1000)){if(GetLastError()==ERROR_SEM_TIMEOUT)continue;fail("debug-event");}DWORD disposition=DBG_CONTINUE;
   if((e.dwDebugEventCode==CREATE_PROCESS_DEBUG_EVENT||e.dwDebugEventCode==LOAD_DLL_DEBUG_EVENT)&&!installed){unsigned char bytes[32];SIZE_T got=0;MEMORY_BASIC_INFORMATION info{};if(VirtualQueryEx(p.hProcess,reinterpret_cast<void *>(setter),&info,sizeof info)&&info.Type==MEM_IMAGE&&ReadProcessMemory(p.hProcess,reinterpret_cast<void *>(setter),bytes,32,&got)&&got==32&&memcmp(bytes,reinterpret_cast<void *>(setter),32)==0){code(p.hProcess,setter,trap);installed=true;}}
   if(e.dwDebugEventCode==CREATE_PROCESS_DEBUG_EVENT){if(e.u.CreateProcessInfo.hFile)CloseHandle(e.u.CreateProcessInfo.hFile);}
   else if(e.dwDebugEventCode==LOAD_DLL_DEBUG_EVENT){if(e.u.LoadDll.hFile)CloseHandle(e.u.LoadDll.hFile);}
   else if(e.dwDebugEventCode==EXCEPTION_DEBUG_EVENT){auto address=uintptr_t(e.u.Exception.ExceptionRecord.ExceptionAddress);ULONG exception=e.u.Exception.ExceptionRecord.ExceptionCode;
    if((exception==EXCEPTION_BREAKPOINT||exception==0x4000001F)&&installed&&address==setter&&!entered){if(e.dwThreadId!=p.dwThreadId)fail("startup-thread");CONTEXT ctx{};ctx.ContextFlags=CONTEXT_CONTROL|CONTEXT_INTEGER;if(!GetThreadContext(p.hThread,&ctx))fail("context");event_arg=ctx.X0;resume=ctx.Lr;read(p.hProcess,resume,saved.data(),4);if(!DuplicateHandle(p.hProcess,reinterpret_cast<HANDLE>(event_arg),GetCurrentProcess(),&event,0,FALSE,DUPLICATE_SAME_ACCESS))fail("event-alias");observe(0);remote=uintptr_t(VirtualAllocEx(p.hProcess,nullptr,4096,MEM_RESERVE|MEM_COMMIT,PAGE_READWRITE));if(!remote)fail("remote");uint64_t values[]={0x41911A28A3BC10B5,0xA5A5A5A5A5A5A5A5};write(p.hProcess,remote,values,16);code(p.hProcess,setter,original);code(p.hProcess,resume,trap);if(after)invoke(ctx,setter,resume,event_arg);else invoke(ctx,subscribe,resume,remote,0,mask,remote+8);if(!SetThreadContext(p.hThread,&ctx))fail("initial-call");entered=true;printf("trap case=%u phase=entry address=%llX\n",ordinal,(unsigned long long)address);}
    else if((exception==EXCEPTION_BREAKPOINT||exception==0x4000001F)&&entered&&address==resume){printf("trap case=%u phase=%u address=%llX\n",ordinal,phase,(unsigned long long)address);CONTEXT ctx{};ctx.ContextFlags=CONTEXT_CONTROL|CONTEXT_INTEGER;if(!GetThreadContext(p.hThread,&ctx))fail("return-context");ULONG status=ULONG(ctx.X0);calls++;observe(phase+1);
     if(phase==0){if(after)set_status=status;else{sub_status=status;read(p.hProcess,remote+8,&id,8);}if(after)invoke(ctx,subscribe,resume,remote,0,mask,remote+8);else invoke(ctx,setter,resume,event_arg);}
     else if(phase==1){if(after){sub_status=status;read(p.hProcess,remote+8,&id,8);}else set_status=status;unsigned char poison[512];memset(poison,0xA5,512);write(p.hProcess,remote+64,poison,512);invoke(ctx,complete,resume,0,0,0,0,remote+64,128);}
     else if(phase==2){get_status[0]=status;read(p.hProcess,remote+64,descriptors[0],48);if(!ResetEvent(event))fail("reset-owned-child-event");invoke(ctx,subscribe,resume,remote,~ULONG(0),next_mask,remote+8);}
     else if(phase==3){repeat_status=status;read(p.hProcess,remote+8,&id2,8);ULONG old_mask=0;memcpy(&old_mask,descriptors[0]+24,4);bool old=get_status[0]==0;invoke(ctx,complete,resume,old?remote+72:0,old?remote+64:0,old?old_mask:0,0,remote+256,128);}
     else if(phase==4){get_status[1]=status;read(p.hProcess,remote+256,descriptors[1],48);invoke(ctx,unsubscribe,resume,remote);}
     else {unsub_status=status;code(p.hProcess,resume,saved);if(!TerminateProcess(p.hProcess,77))fail("terminate-child");}
     if(phase<5&&!SetThreadContext(p.hThread,&ctx))fail("next-call");phase++;
    }else if(exception!=EXCEPTION_BREAKPOINT&&exception!=0x4000001F){printf("unexpected=%08lX address=%llX\n",exception,(unsigned long long)address);disposition=DBG_EXCEPTION_NOT_HANDLED;}
   }else if(e.dwDebugEventCode==EXIT_PROCESS_DEBUG_EVENT){exit=e.u.ExitProcess.dwExitCode;exited=true;}
   if(!ContinueDebugEvent(e.dwProcessId,e.dwThreadId,disposition))fail("continue");
  }
  if(!exited){TerminateProcess(p.hProcess,78);fail("deadline");}if(event)CloseHandle(event);CloseHandle(p.hThread);CloseHandle(p.hProcess);GetProcessHandleCount(GetCurrentProcess(),&ha);
  printf("case=%u warmup=%u width=%zu after=%u mask=%08X next-mask=%08X calls=%u registered=%08X status=%08X id=%016llX repeat=%08X repeat-id=%016llX end=%08X state0=%ld state1=%ld state2=%ld state3=%ld state4=%ld type0=%lu type1=%lu type2=%lu type3=%lu type4=%lu exit=%lu handles-before=%lu handles-after=%lu\n",ordinal,warmup,sizeof(void *),after,mask,next_mask,calls,set_status,sub_status,(unsigned long long)id,repeat_status,(unsigned long long)id2,unsub_status,states[0],states[1],states[2],states[3],states[4],types[0],types[1],types[2],types[3],types[4],exit,hb,ha);
  for(unsigned d=0;d<2;d++){printf("descriptor case=%u index=%u status=%08X state=%ld bytes=",ordinal,d,get_status[d],states[d==0?3:5]);for(unsigned i=0;i<48;i++)printf("%02X",descriptors[d][i]);puts("");}
  if(calls!=6||exit!=77||(!warmup&&(hb!=ha||before_handles!=handles())))return 22;
 }
 puts("complete cases=128 warmup=1");return 0;
}
