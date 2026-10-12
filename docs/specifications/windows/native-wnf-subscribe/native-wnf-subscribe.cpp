#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <cstdint>
#include <cstdio>
#include <initializer_list>
using Subscribe=LONG(NTAPI *)(const uint64_t *,ULONG,ULONG,uint64_t *);
using Unsubscribe=LONG(NTAPI *)(const uint64_t *);
using Info=LONG(NTAPI *)(const uint64_t *,ULONG,void *,void *,ULONG);
int main(){
 setvbuf(stdout,nullptr,_IONBF,0);auto module=GetModuleHandleW(L"ntdll.dll");auto sub=reinterpret_cast<Subscribe>(GetProcAddress(module,"NtSubscribeWnfStateChange"));auto unsub=reinterpret_cast<Unsubscribe>(GetProcAddress(module,"NtUnsubscribeWnfStateChange"));auto info=reinterpret_cast<Info>(GetProcAddress(module,"NtQueryWnfStateNameInformation"));if(!sub||!unsub||!info)return 2;
 unsigned ordinal=0;constexpr uint64_t original=0x41911A28A3BC10B5;
 for(uint64_t name:{original,0ull,~0ull,original^1,original^0x10,original^0x40})for(ULONG stamp:{0u,1u,0xFFFFFFFFu})for(ULONG mask:{0u,1u,0x10u,0x11u,0xFFFFFFFFu}){
  DWORD before=0,after=0;if(!GetProcessHandleCount(GetCurrentProcess(),&before))return 3;
  LONG clear=unsub(&name);ULONG present_before=0xA5A5A5A5,present_after=present_before;LONG qb=info(&name,1,nullptr,&present_before,4);
  uint64_t id=0xA5A5A5A5A5A5A5A5,repeat_id=id;LONG status=sub(&name,stamp,mask,&id),again=sub(&name,stamp,mask,&repeat_id);LONG qa=info(&name,1,nullptr,&present_after,4);LONG end=unsub(&name);if(!GetProcessHandleCount(GetCurrentProcess(),&after))return 4;
  printf("case=%u width=%zu name=%016llX stamp=%08lX mask=%08lX clear=%08lX status=%08lX id=%016llX repeat=%08lX repeat-id=%016llX query-before=%08lX present-before=%08lX query-after=%08lX present-after=%08lX end=%08lX handles-before=%lu handles-after=%lu\n",ordinal++,sizeof(void *),(unsigned long long)name,stamp,mask,ULONG(clear),ULONG(status),(unsigned long long)id,ULONG(again),(unsigned long long)repeat_id,ULONG(qb),present_before,ULONG(qa),present_after,ULONG(end),before,after);
  if(before!=after)return 5;
 }
 printf("complete cases=%u\n",ordinal);return 0;
}
