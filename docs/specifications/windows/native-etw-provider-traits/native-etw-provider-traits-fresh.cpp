#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <winternl.h>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>
#if !defined(_M_ARM64)
#error This producer verifies native ARM64 entry and true LR return.
#endif
static void fail(const char* what){printf("failure=%s error=%lu\n",what,GetLastError());ExitProcess(21);}
static void read(HANDLE p,uintptr_t at,void* out,SIZE_T n){SIZE_T got=0;if(!ReadProcessMemory(p,reinterpret_cast<void*>(at),out,n,&got)||got!=n)fail("read-child");}
static void write(HANDLE p,uintptr_t at,const void* in,SIZE_T n){SIZE_T got=0;if(!WriteProcessMemory(p,reinterpret_cast<void*>(at),in,n,&got)||got!=n)fail("write-child");}
static void code(HANDLE p,uintptr_t at,const unsigned char* bytes){DWORD old=0,ignored=0;if(!VirtualProtectEx(p,reinterpret_cast<void*>(at),4,PAGE_EXECUTE_READWRITE,&old))fail("protect-child-code");write(p,at,bytes,4);if(!VirtualProtectEx(p,reinterpret_cast<void*>(at),4,old,&ignored)||!FlushInstructionCache(p,reinterpret_cast<void*>(at),4))fail("restore-child-code");}
static std::vector<uintptr_t> handles(){std::vector<uintptr_t> v;for(uintptr_t h=4;h<65536;h+=4){DWORD f=0;if(GetHandleInformation(reinterpret_cast<HANDLE>(h),&f))v.push_back(h);}return v;}
static void dump(HANDLE p,uintptr_t at,ULONG n,unsigned ordinal,const char* phase){if(n>4096)fail("dump-bound");std::vector<unsigned char>b(n);if(n)read(p,at,b.data(),n);printf("bytes case=%u phase=%s address=%llX size=%lu data=",ordinal,phase,(unsigned long long)at,n);for(auto x:b)printf("%02X",unsigned(x));puts("");}
int main(int argc,char**){if(argc>1)return 41;setvbuf(stdout,nullptr,_IONBF,0);
 auto module=GetModuleHandleW(L"ntdll.dll");uintptr_t entry=uintptr_t(GetProcAddress(module,"NtTraceControl"));auto dbg=GetProcAddress(module,"DbgBreakPoint");if(!entry||!dbg)fail("exports");unsigned char trap[4],original[4];memcpy(trap,dbg,4);memcpy(original,reinterpret_cast<void*>(entry),4);ULONG encoded=0;memcpy(&encoded,trap,4);if((encoded&0xFFE0001F)!=0xD4200000)fail("native-break-instruction");
 wchar_t executable[32768];if(!GetModuleFileNameW(nullptr,executable,32768))fail("executable-path");
 for(unsigned ordinal=0;ordinal<3;ordinal++){
  auto hb=handles();DWORD count_before=0,count_after=0;if(!GetProcessHandleCount(GetCurrentProcess(),&count_before))fail("handle-count-before");
  std::wstring command=L"\""+std::wstring(executable)+L"\" --child";STARTUPINFOW start{};start.cb=sizeof(start);PROCESS_INFORMATION p{};
  if(!CreateProcessW(executable,command.data(),nullptr,nullptr,FALSE,DEBUG_ONLY_THIS_PROCESS,nullptr,nullptr,&start,&p))fail("debug-child");
  bool installed=false,waiting=false,done=false,exited=false;uintptr_t resume=0;unsigned char caller_bytes[4];ULONG function=0,exit_code=0;ULONGLONG args[6]{};unsigned calls=0;ULONGLONG deadline=GetTickCount64()+30000;
  while(!exited&&GetTickCount64()<deadline){DEBUG_EVENT e{};if(!WaitForDebugEvent(&e,1000)){if(GetLastError()==ERROR_SEM_TIMEOUT)continue;fail("wait-debug-event");}DWORD disposition=DBG_CONTINUE;
   if((e.dwDebugEventCode==CREATE_PROCESS_DEBUG_EVENT||e.dwDebugEventCode==LOAD_DLL_DEBUG_EVENT)&&!installed){MEMORY_BASIC_INFORMATION info{};unsigned char bytes[32];SIZE_T got=0;if(VirtualQueryEx(p.hProcess,reinterpret_cast<void*>(entry),&info,sizeof(info))&&info.Type==MEM_IMAGE&&ReadProcessMemory(p.hProcess,reinterpret_cast<void*>(entry),bytes,32,&got)&&got==32&&memcmp(bytes,reinterpret_cast<void*>(entry),32)==0){code(p.hProcess,entry,trap);installed=true;printf("image case=%u entry=%llX allocation=%llX type=%lX bytes=",ordinal,(unsigned long long)entry,(unsigned long long)info.AllocationBase,info.Type);for(auto b:bytes)printf("%02X",unsigned(b));puts("");}}
   if(e.dwDebugEventCode==CREATE_PROCESS_DEBUG_EVENT){if(e.u.CreateProcessInfo.hFile)CloseHandle(e.u.CreateProcessInfo.hFile);}
   else if(e.dwDebugEventCode==LOAD_DLL_DEBUG_EVENT){if(e.u.LoadDll.hFile)CloseHandle(e.u.LoadDll.hFile);}
   else if(e.dwDebugEventCode==EXCEPTION_DEBUG_EVENT){auto address=uintptr_t(e.u.Exception.ExceptionRecord.ExceptionAddress);ULONG exception=e.u.Exception.ExceptionRecord.ExceptionCode;bool breakpoint=exception==EXCEPTION_BREAKPOINT||exception==0x4000001F||exception==EXCEPTION_ILLEGAL_INSTRUCTION;
    if(breakpoint&&installed&&!waiting&&!done&&address==entry){if(e.dwThreadId!=p.dwThreadId)fail("registration-thread");unsigned char actual[4];read(p.hProcess,entry,actual,4);if(memcmp(actual,trap,4))fail("entry-trap-bytes");CONTEXT c{};c.ContextFlags=CONTEXT_CONTROL|CONTEXT_INTEGER;if(!GetThreadContext(p.hThread,&c))fail("entry-context");for(unsigned i=0;i<6;i++)args[i]=c.X[i];function=ULONG(args[0]);resume=c.Lr;MEMORY_BASIC_INFORMATION info{};if(!VirtualQueryEx(p.hProcess,reinterpret_cast<void*>(resume),&info,sizeof(info))||info.Type!=MEM_IMAGE)fail("true-return-image");read(p.hProcess,resume,caller_bytes,4);
     printf("entry case=%u call=%u function=%lu thread=%lu pc=%llX lr=%llX args=",ordinal,calls,function,e.dwThreadId,(unsigned long long)c.Pc,(unsigned long long)resume);for(auto a:args)printf("%llX,",a);puts("");
     if(function==30){if(args[2]!=24||args[4]<120||!args[1]||!args[3]||!args[5])fail("initial-provider-traits-shape");dump(p.hProcess,args[1],ULONG(args[2]),ordinal,"input-before");dump(p.hProcess,args[3],ULONG(args[4]),ordinal,"output-before");dump(p.hProcess,args[5],4,ordinal,"returned-before");}
     code(p.hProcess,entry,original);code(p.hProcess,resume,trap);c.Pc=entry;if(!SetThreadContext(p.hThread,&c))fail("entry-resume");waiting=true;
    }else if(breakpoint&&waiting&&address==resume){unsigned char actual[4];read(p.hProcess,resume,actual,4);if(memcmp(actual,trap,4))fail("return-trap-bytes");CONTEXT c{};c.ContextFlags=CONTEXT_CONTROL|CONTEXT_INTEGER;if(!GetThreadContext(p.hThread,&c))fail("return-context");printf("return case=%u call=%u function=%lu thread=%lu pc=%llX status=%08lX\n",ordinal,calls,function,e.dwThreadId,(unsigned long long)c.Pc,ULONG(c.X0));code(p.hProcess,resume,caller_bytes);calls++;waiting=false;
     if(function==30){dump(p.hProcess,args[1],ULONG(args[2]),ordinal,"input-after");dump(p.hProcess,args[3],ULONG(args[4]),ordinal,"output-after");dump(p.hProcess,args[5],4,ordinal,"returned-after");done=true;if(!TerminateProcess(p.hProcess,77))fail("terminate-owned-child");}
     else {if(calls>32)fail("call-bound");code(p.hProcess,entry,trap);c.Pc=resume;if(!SetThreadContext(p.hThread,&c))fail("skip-return-resume");}
    }else if(exception!=EXCEPTION_BREAKPOINT&&exception!=0x4000001F){printf("unexpected-exception=%08lX address=%llX\n",exception,(unsigned long long)address);disposition=DBG_EXCEPTION_NOT_HANDLED;}
   }else if(e.dwDebugEventCode==EXIT_PROCESS_DEBUG_EVENT){exit_code=e.u.ExitProcess.dwExitCode;exited=true;}
   if(!ContinueDebugEvent(e.dwProcessId,e.dwThreadId,disposition))fail("continue-debug-event");
  }
  if(!exited){TerminateProcess(p.hProcess,78);fail("capture-deadline");}CloseHandle(p.hThread);CloseHandle(p.hProcess);if(!GetProcessHandleCount(GetCurrentProcess(),&count_after))fail("handle-count-after");printf("case=%u warmup=%u installed=%u done=%u calls=%u exit=%lu handles-before=%lu handles-after=%lu exact-handle-set=%u\n",ordinal,ordinal==0,installed,done,calls,exit_code,count_before,count_after,hb==handles());if(!installed||!done||exit_code!=77||(ordinal&&hb!=handles()))return 22;
 }
 puts("complete cases=2 warmup=1");return 0;
}
