#include <windows.h>
#include <winternl.h>
#include <cstdio>
#include <initializer_list>
using Timer=LONG(NTAPI*)(PHANDLE,ACCESS_MASK,POBJECT_ATTRIBUTES,ULONG);
using Timer2=LONG(NTAPI*)(PHANDLE,PULONG,POBJECT_ATTRIBUTES,ULONG,ACCESS_MASK);
static unsigned ordinal;
static void timers(HMODULE n){
 auto legacy=reinterpret_cast<Timer>(GetProcAddress(n,"NtCreateTimer"));
 auto extended=reinterpret_cast<Timer2>(GetProcAddress(n,"NtCreateTimer2"));
 if(!legacy||!extended)ExitProcess(30);
 for(unsigned family:{0u,1u})for(ULONG flag:family?std::initializer_list<ULONG>{0,8,0x80000008}:std::initializer_list<ULONG>{0,1}){
  DWORD before=0,after=0;GetProcessHandleCount(GetCurrentProcess(),&before);HANDLE h=nullptr;
  LONG status=family?extended(&h,nullptr,nullptr,flag,0x1F0003):legacy(&h,0x1F0003,nullptr,flag);
  if(status<0||!h)ExitProcess(31);
  SetLastError(0);DWORD first=WaitForSingleObject(h,0),error=GetLastError();SetLastError(0);DWORD second=WaitForSingleObject(h,0),second_error=GetLastError();
  BOOL closed=CloseHandle(h);GetProcessHandleCount(GetCurrentProcess(),&after);
  std::printf("case=%u kind=timer family=%u flag=%08lX width=%zu status=%08lX first=%08lX error=%lu second=%08lX second-error=%lu close=%u before=%lu after=%lu\n",ordinal++,family,flag,sizeof(void*),ULONG(status),first,error,second,second_error,closed,before,after);
  if(!closed||before!=after)ExitProcess(32);
 }
}
static void events(){
 for(BOOL manual:{FALSE,TRUE}){
  DWORD before=0,after=0;GetProcessHandleCount(GetCurrentProcess(),&before);
  HANDLE h=CreateEventW(nullptr,manual,TRUE,nullptr),d=nullptr;
  if(!h||!DuplicateHandle(GetCurrentProcess(),h,GetCurrentProcess(),&d,0,FALSE,DUPLICATE_SAME_ACCESS))ExitProcess(33);
  HANDLE pair[]{h,d};SetLastError(0);DWORD all=WaitForMultipleObjects(2,pair,TRUE,0),all_error=GetLastError();
  SetLastError(0);DWORD any=WaitForMultipleObjects(2,pair,FALSE,0),any_error=GetLastError();SetLastError(0);DWORD single=WaitForSingleObject(d,0),single_error=GetLastError();
  BOOL first_close=CloseHandle(h),second_close=CloseHandle(d);GetProcessHandleCount(GetCurrentProcess(),&after);
  std::printf("case=%u kind=event manual=%u width=%zu all=%08lX all-error=%lu any=%08lX any-error=%lu single=%08lX single-error=%lu first-close=%u second-close=%u before=%lu after=%lu\n",ordinal++,manual,sizeof(void*),all,all_error,any,any_error,single,single_error,first_close,second_close,before,after);
  if(!first_close||!second_close||before!=after)ExitProcess(34);
 }
}
int main(){timers(GetModuleHandleW(L"ntdll.dll"));events();std::printf("complete cases=%u\n",ordinal);return 0;}
