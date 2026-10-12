#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <initializer_list>
using Control=LONG(NTAPI*)(ULONG,void*,ULONG,void*,ULONG,ULONG*);using Close=LONG(NTAPI*)(HANDLE);
static BOOL duplicate_call(HANDLE h,HANDLE* out,DWORD grant,DWORD options,DWORD* exception){*exception=0;__try{return DuplicateHandle(GetCurrentProcess(),h,GetCurrentProcess(),out,grant,FALSE,options);}__except(EXCEPTION_EXECUTE_HANDLER){*exception=GetExceptionCode();return FALSE;}}
int main(){setvbuf(stdout,nullptr,_IONBF,0);auto n=GetModuleHandleW(L"ntdll.dll");auto control=reinterpret_cast<Control>(GetProcAddress(n,"NtTraceControl"));auto close_nt=reinterpret_cast<Close>(GetProcAddress(n,"NtClose"));if(!control||!close_nt)return 2;SYSTEM_INFO si{};GetSystemInfo(&si);SIZE_T page=si.dwPageSize;auto memory=static_cast<unsigned char*>(VirtualAlloc(nullptr,2*page,MEM_RESERVE|MEM_COMMIT,PAGE_READWRITE));if(!memory)return 3;unsigned ordinal=0;
 for(unsigned kind=0;kind<5;kind++)for(unsigned offset=0;offset<4;offset++)for(DWORD options:{0u,1u,2u,3u})for(DWORD grant:{0u,0xFFFFFFFFu}){
  DWORD old=0;VirtualProtect(memory,2*page,PAGE_READWRITE,&old);memset(memory,0xA5,2*page);unsigned char input[160]{},output[160];ULONG type=3,flag=1,returned=0;memcpy(input+16,&type,4);memcpy(input+20,&flag,4);DWORD hb=0,ha=0;GetProcessHandleCount(GetCurrentProcess(),&hb);LONG s=control(15,input,160,output,160,&returned);uint64_t h=0;memcpy(&h,output+24,8);if(s<0||!h)return 4;auto address=memory+(kind==4?page-sizeof(void*)+2:offset);if(kind>=1&&kind<=3)VirtualProtect(memory,page,kind==1?PAGE_READONLY:kind==2?PAGE_NOACCESS:PAGE_READWRITE|PAGE_GUARD,&old);if(kind==4)VirtualProtect(memory+page,page,PAGE_NOACCESS,&old);
  SetLastError(0xB6B6B6B6);DWORD exception=0;BOOL ok=duplicate_call(reinterpret_cast<HANDLE>(uintptr_t(h)),reinterpret_cast<HANDLE*>(address),grant,options,&exception);DWORD error=GetLastError(),flags=0;BOOL live=GetHandleInformation(reinterpret_cast<HANDLE>(uintptr_t(h)),&flags);MEMORY_BASIC_INFORMATION info{};VirtualQuery(memory,&info,sizeof(info));VirtualProtect(memory,2*page,PAGE_READWRITE,&old);uint64_t value=0;memcpy(&value,address,sizeof(void*));if(live&&close_nt(reinterpret_cast<HANDLE>(uintptr_t(h)))<0)return 5;GetProcessHandleCount(GetCurrentProcess(),&ha);
  printf("case=%u width=%zu kind=%u offset=%u options=%lu grant=%08lX ok=%u error=%08lX exception=%08lX output=%llX source-live=%u protect=%08lX handles-before=%lu handles-after=%lu\n",ordinal++,sizeof(void*),kind,offset,options,grant,ok,error,exception,(unsigned long long)value,live,info.Protect,hb,ha);if(hb!=ha)return 6;
 }
 VirtualFree(memory,0,MEM_RELEASE);printf("complete cases=%u\n",ordinal);return 0;
}
