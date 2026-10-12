#include <windows.h>
#include <winternl.h>
#include <cstdio>
#include <initializer_list>
using Create=LONG(NTAPI*)(PHANDLE,ACCESS_MASK,POBJECT_ATTRIBUTES);
using Close=LONG(NTAPI*)(HANDLE);
using Wait=LONG(NTAPI*)(HANDLE,BOOLEAN,PLARGE_INTEGER);
static Create create_packet;static Close close_object;static Wait wait_object;
static unsigned ordinal;
static LONG ntwait(HANDLE h){LARGE_INTEGER zero{};return wait_object(h,FALSE,&zero);}
static void run(ULONG access,BOOL inherit){
 DWORD before=0,after=0;GetProcessHandleCount(GetCurrentProcess(),&before);
 OBJECT_ATTRIBUTES oa{};oa.Length=sizeof oa;oa.Attributes=inherit?2:0;
 HANDLE h=nullptr,duplicate=nullptr;LONG status=create_packet(&h,access,&oa);
 if(status<0||!h)ExitProcess(20);
 LONG first=ntwait(h),second=ntwait(h),third=ntwait(h);
 SetLastError(0);DWORD single=WaitForSingleObject(h,0),single_error=GetLastError();
 SetLastError(0);BOOL copied=DuplicateHandle(GetCurrentProcess(),h,GetCurrentProcess(),&duplicate,0,FALSE,DUPLICATE_SAME_ACCESS);DWORD duplicate_error=GetLastError();
 if(!copied)ExitProcess(21);
 LONG alias_first=ntwait(duplicate),alias_second=ntwait(duplicate);
 HANDLE aliases[2]{h,duplicate};SetLastError(0);DWORD any=WaitForMultipleObjects(2,aliases,FALSE,0),any_error=GetLastError();
 SetLastError(0);DWORD all=WaitForMultipleObjects(2,aliases,TRUE,0),all_error=GetLastError();
 LONG after_multiple=ntwait(h),close_first=close_object(h),after_close=ntwait(duplicate),close_last=close_object(duplicate);
 GetProcessHandleCount(GetCurrentProcess(),&after);
 std::printf("case=%u width=%zu access=%08lX inherit=%u status=%08lX first=%08lX second=%08lX third=%08lX single=%08lX single-error=%lu duplicate-error=%lu alias-first=%08lX alias-second=%08lX any=%08lX any-error=%lu all=%08lX all-error=%lu after-multiple=%08lX close-first=%08lX after-close=%08lX close-last=%08lX handles-before=%lu handles-after=%lu\n",ordinal++,sizeof(void*),access,inherit,ULONG(status),ULONG(first),ULONG(second),ULONG(third),single,single_error,duplicate_error,ULONG(alias_first),ULONG(alias_second),any,any_error,all,all_error,ULONG(after_multiple),ULONG(close_first),ULONG(after_close),ULONG(close_last),before,after);
 if(close_first<0||close_last<0||before!=after)ExitProcess(22);
}
int main(){
 HMODULE ntdll=GetModuleHandleW(L"ntdll.dll");
 create_packet=reinterpret_cast<Create>(GetProcAddress(ntdll,"NtCreateWaitCompletionPacket"));
 close_object=reinterpret_cast<Close>(GetProcAddress(ntdll,"NtClose"));
 wait_object=reinterpret_cast<Wait>(GetProcAddress(ntdll,"NtWaitForSingleObject"));
 if(!create_packet||!close_object||!wait_object)return 23;
 for(ULONG access:{0ul,1ul,0x100000ul,0x100001ul,0x1F0001ul,0xF0001ul,0x2000000ul,0x80000000ul,0x10000000ul})for(BOOL inherit:{FALSE,TRUE})run(access,inherit);
 std::printf("complete cases=%u\n",ordinal);return 0;
}
