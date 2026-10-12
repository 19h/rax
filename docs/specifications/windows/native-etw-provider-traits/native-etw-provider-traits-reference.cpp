#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <initializer_list>
using Control=LONG(NTAPI*)(ULONG,void*,ULONG,void*,ULONG,ULONG*);using Close=LONG(NTAPI*)(HANDLE);using Translate=ULONG(NTAPI*)(LONG);
int main(){setvbuf(stdout,nullptr,_IONBF,0);auto n=GetModuleHandleW(L"ntdll.dll");auto control=reinterpret_cast<Control>(GetProcAddress(n,"NtTraceControl"));auto close_nt=reinterpret_cast<Close>(GetProcAddress(n,"NtClose"));auto translate=reinterpret_cast<Translate>(GetProcAddress(n,"RtlNtStatusToDosError"));if(!control||!close_nt||!translate)return 2;SYSTEM_INFO si{};GetSystemInfo(&si);SIZE_T page=si.dwPageSize;auto memory=static_cast<unsigned char*>(VirtualAlloc(nullptr,20*page,MEM_RESERVE|MEM_COMMIT,PAGE_READWRITE));if(!memory)return 3;unsigned count=0;
 for(unsigned busy=0;busy<2;busy++)for(unsigned hm=0;hm<5;hm++)for(unsigned tk=0;tk<11;tk++)for(ULONG length:{0u,1u,2u,3u,7u,4096u,65535u,65536u,0xFFFFFFFFu}){
  DWORD old=0;VirtualProtect(memory,20*page,PAGE_READWRITE,&old);memset(memory,0,20*page);unsigned char valid[3]={3,0,0},reg[160]{},outreg[160],input[24]{},output[120];memset(output,0xA5,120);ULONG kind=3,flag=1,regret=0;memcpy(reg+16,&kind,4);memcpy(reg+20,&flag,4);DWORD hb=0,ha=0;GetProcessHandleCount(GetCurrentProcess(),&hb);if(control(15,reg,160,outreg,160,&regret)<0)return 4;uint64_t own=0;memcpy(&own,outreg+24,8);uint64_t h=own;HANDLE event=nullptr;
  if(busy){uint64_t pointer=uintptr_t(valid);ULONG three=3;memcpy(input,&h,8);memcpy(input+8,&pointer,8);memcpy(input+16,&three,4);ULONG r=0;if(control(30,input,24,output,120,&r)<0)return 5;}
  if(hm==1)h=0;else if(hm==2){event=CreateEventW(nullptr,FALSE,FALSE,nullptr);if(!event)return 6;h=uintptr_t(event);}else if(hm==3){if(close_nt(reinterpret_cast<HANDLE>(uintptr_t(own)))<0)return 7;}else if(hm==4)h|=uint64_t(1)<<32;
  auto address=memory+(tk==6?page-2:tk>=7&&tk<=9?tk-6:0);USHORT embedded=USHORT(length);memcpy(address,&embedded,2);if(length>=3&&length<=65535)address[length-1]=0;
  if(tk>=3&&tk<=5)VirtualProtect(memory,page,tk==3?PAGE_READWRITE|PAGE_GUARD:tk==4?PAGE_NOACCESS:PAGE_READONLY,&old);if(tk==6)VirtualProtect(memory+page,page,PAGE_NOACCESS,&old);
  uint64_t pointer=tk==1?0:tk==2?1:uintptr_t(address);if(tk==10)pointer|=uint64_t(1)<<48;
  memcpy(input,&h,8);memcpy(input+8,&pointer,8);memcpy(input+16,&length,4);ULONG returned=0xA5A5A5A5;LONG status=control(30,input,24,output,120,&returned);MEMORY_BASIC_INFORMATION q{};VirtualQuery(memory,&q,sizeof(q));VirtualProtect(memory,20*page,PAGE_READWRITE,&old);ULONG err=translate(status);
  uint64_t clean_pointer=uintptr_t(valid);ULONG three=3;memcpy(input,&own,8);memcpy(input+8,&clean_pointer,8);memcpy(input+16,&three,4);ULONG retry_ret=0xA5A5A5A5;LONG retry=control(30,input,24,output,120,&retry_ret);LONG close=LONG(0xDEADBEEF);if(hm!=3)close=close_nt(reinterpret_cast<HANDLE>(uintptr_t(own)));if(event)CloseHandle(event);GetProcessHandleCount(GetCurrentProcess(),&ha);
  printf("case=%u width=%zu busy=%u handle-mode=%u traits=%u length=%lu status=%08lX returned=%08lX error=%lu protect=%08lX retry=%08lX retry-returned=%08lX close=%08lX handles-before=%lu handles-after=%lu\n",count++,sizeof(void*),busy,hm,tk,length,ULONG(status),returned,err,q.Protect,ULONG(retry),retry_ret,ULONG(close),hb,ha);if(hb!=ha)return 8;for(auto v:output)if(v!=0xA5)return 9;
 }
 VirtualFree(memory,0,MEM_RELEASE);printf("complete cases=%u\n",count);return 0;
}
