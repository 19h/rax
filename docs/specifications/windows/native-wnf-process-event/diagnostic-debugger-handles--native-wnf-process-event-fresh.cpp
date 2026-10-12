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

static std::vector<uintptr_t> handles(const char *phase,unsigned mode){
 auto query=reinterpret_cast<Query>(GetProcAddress(GetModuleHandleW(L"ntdll.dll"),"NtQueryObject"));std::vector<uintptr_t> result;
 for(uintptr_t value=4;value<65536;value+=4){DWORD flags=0;if(GetHandleInformation(reinterpret_cast<HANDLE>(value),&flags)){result.push_back(value);unsigned char buffer[2048]{};ULONG returned=0;LONG status=query(reinterpret_cast<HANDLE>(value),2,buffer,sizeof buffer,&returned);auto name=reinterpret_cast<UNICODE_STRING *>(buffer);printf("handle phase=%s mode=%u value=%llX query=%08lX type=%.*ls\n",phase,mode,(unsigned long long)value,ULONG(status),status>=0?name->Length/2:0,status>=0?name->Buffer:L"");}}
 return result;
}
int main(int argc,char **argv){
 if(argc>1)return 41;setvbuf(stdout,nullptr,_IONBF,0);
 auto module=GetModuleHandleW(L"ntdll.dll");auto entry=uintptr_t(GetProcAddress(module,"NtSetWnfProcessNotificationEvent"));auto dbg=GetProcAddress(module,"DbgBreakPoint");auto query=reinterpret_cast<Query>(GetProcAddress(module,"NtQueryEvent"));if(!entry||!dbg||!query)fail("exports");
 SIZE_T instruction=sizeof(void *)==8?4:1;
#if !defined(_M_ARM64)
 instruction=1;
#endif
 std::vector<unsigned char> trap(instruction),original(instruction);memcpy(trap.data(),dbg,instruction);memcpy(original.data(),reinterpret_cast<void *>(entry),instruction);
#if defined(_M_ARM64)
 ULONG encoded=0;memcpy(&encoded,trap.data(),4);if((encoded&0xFFE0001F)!=0xD4200000)fail("native-break-instruction");
#else
 if(trap[0]!=0xCC)fail("native-break-instruction");
#endif
 wchar_t executable[32768];if(!GetModuleFileNameW(nullptr,executable,32768))fail("executable-path");
 for(unsigned mode=0;mode<3;mode++){
  auto before_handles=handles("before",mode);DWORD hb=0,ha=0;if(!GetProcessHandleCount(GetCurrentProcess(),&hb))fail("handle-count-before");
  std::wstring command=L"\""+std::wstring(executable)+L"\" --child";STARTUPINFOW start{};start.cb=sizeof start;PROCESS_INFORMATION process{};
  if(!CreateProcessW(executable,command.data(),nullptr,nullptr,FALSE,DEBUG_ONLY_THIS_PROCESS,nullptr,nullptr,&start,&process))fail("debug-child");
  bool installed=false,entered=false,exited=false;unsigned calls=0;uintptr_t resume=0,real_arg=0,selected=0;std::vector<unsigned char> caller_bytes(instruction);HANDLE parent_event=nullptr,child_event=nullptr;EventInfo before{},after{};ULONG rb=0,ra=0;LONG qb=LONG(0xDEADBEEF),qa=qb;ULONG first=0xDEADBEEF,second=first,exit_code=0;ULONGLONG deadline=GetTickCount64()+30000;
  if(mode==2){parent_event=CreateEventW(nullptr,TRUE,TRUE,nullptr);if(!parent_event||!DuplicateHandle(GetCurrentProcess(),parent_event,process.hProcess,&child_event,0,FALSE,DUPLICATE_SAME_ACCESS))fail("provided-child-event");}
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
    if(exception==EXCEPTION_BREAKPOINT&&installed&&address==entry&&!entered){
     if(event.dwThreadId!=process.dwThreadId)fail("first-registration-thread");CONTEXT c{};c.ContextFlags=CONTEXT_CONTROL|CONTEXT_INTEGER;if(!GetThreadContext(process.hThread,&c))fail("entry-context");
     real_arg=argument(process.hProcess,c);selected=mode==0?real_arg:mode==1?0:uintptr_t(child_event);resume=caller(process.hProcess,c);read(process.hProcess,resume,caller_bytes.data(),instruction);
     if(mode!=2&&!DuplicateHandle(process.hProcess,reinterpret_cast<HANDLE>(real_arg),GetCurrentProcess(),&parent_event,0,FALSE,DUPLICATE_SAME_ACCESS))fail("original-event-alias");
     qb=query(parent_event,0,&before,8,&rb);code(process.hProcess,entry,original);code(process.hProcess,resume,trap);set_pc(c,entry);set_argument(process.hProcess,c,selected);if(!SetThreadContext(process.hThread,&c))fail("entry-resume-context");entered=true;
    }else if(exception==EXCEPTION_BREAKPOINT&&entered&&address==resume){
     CONTEXT c{};c.ContextFlags=CONTEXT_CONTROL|CONTEXT_INTEGER;if(!GetThreadContext(process.hThread,&c))fail("return-context");
     if(calls==0){first=ULONG(result(c));calls++;repeat(process.hProcess,c,entry,resume,selected);if(!SetThreadContext(process.hThread,&c))fail("repeat-context");}
     else{second=ULONG(result(c));calls++;qa=query(parent_event,0,&after,8,&ra);code(process.hProcess,resume,caller_bytes);if(!TerminateProcess(process.hProcess,77))fail("terminate-owned-child");}
    }else if(exception!=EXCEPTION_BREAKPOINT&&exception!=0x4000001F){printf("unexpected-exception=%08lX address=%llX mode=%u\n",exception,(unsigned long long)address,mode);disposition=DBG_EXCEPTION_NOT_HANDLED;}
   }else if(event.dwDebugEventCode==EXIT_PROCESS_DEBUG_EVENT){exit_code=event.u.ExitProcess.dwExitCode;exited=true;}
   if(!ContinueDebugEvent(event.dwProcessId,event.dwThreadId,disposition))fail("continue-debug-event");
  }
  if(!exited){TerminateProcess(process.hProcess,78);fail("capture-deadline");}
  if(parent_event)CloseHandle(parent_event);CloseHandle(process.hThread);CloseHandle(process.hProcess);if(!GetProcessHandleCount(GetCurrentProcess(),&ha))fail("handle-count-after");
  printf("case=%u width=%zu mode=%u child=%lu installed=%u entered=%u calls=%u entry=%llX caller=%llX original=%llX selected=%llX status=%08lX repeat=%08lX query-before=%08lX returned-before=%lu type-before=%lu state-before=%ld query-after=%08lX returned-after=%lu type-after=%lu state-after=%ld exit=%lu handles-before=%lu handles-after=%lu\n",mode,sizeof(void *),mode,process.dwProcessId,installed,entered,calls,(unsigned long long)entry,(unsigned long long)resume,(unsigned long long)real_arg,(unsigned long long)selected,first,second,ULONG(qb),rb,before.type,before.state,ULONG(qa),ra,after.type,after.state,exit_code,hb,ha);
  auto after_handles=handles("after",mode);if(!installed||!entered||calls!=2||exit_code!=77||qb<0||qa<0)return 22;
 }
 puts("complete cases=3");return 0;
}
