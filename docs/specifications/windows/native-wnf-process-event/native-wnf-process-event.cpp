#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <winternl.h>
#include <cstdio>
#include <cstdint>
#include <cstring>
#include <initializer_list>
using Set=LONG(NTAPI *)(HANDLE);
using Query=LONG(NTAPI *)(HANDLE,ULONG,PVOID,ULONG,PULONG);
using Close=LONG(NTAPI *)(HANDLE);
struct EventInfo{ULONG type;LONG state;};
int main(){
 setvbuf(stdout,nullptr,_IONBF,0);auto n=GetModuleHandleW(L"ntdll.dll");auto set=reinterpret_cast<Set>(GetProcAddress(n,"NtSetWnfProcessNotificationEvent"));auto query=reinterpret_cast<Query>(GetProcAddress(n,"NtQueryEvent"));auto close=reinterpret_cast<Close>(GetProcAddress(n,"NtClose"));if(!set||!query||!close)return 2;
 unsigned ordinal=0;
 for(unsigned manual:{0u,1u})for(unsigned initial:{0u,1u})for(unsigned mode=0;mode<12;mode++){
  DWORD hb=0,ha=0;if(!GetProcessHandleCount(GetCurrentProcess(),&hb))return 3;
  LONG reset=set(nullptr);HANDLE event=CreateEventW(nullptr,manual,initial,nullptr),alias=nullptr,other=nullptr;if(!event)return 4;HANDLE selected=event;
  if(mode==1)selected=nullptr;else if(mode==2)selected=reinterpret_cast<HANDLE>(uintptr_t(0x13572468));
  else if(mode==3)selected=GetCurrentProcess();else if(mode==4)selected=GetCurrentThread();
  else if(mode>=5&&mode<=10){ULONG grant[]={0,1,2,3,0x100000,0x1F0003};if(!DuplicateHandle(GetCurrentProcess(),event,GetCurrentProcess(),&alias,grant[mode-5],FALSE,0))return 5;selected=alias;}
  else if(mode==11){other=CreateSemaphoreW(nullptr,0,1,nullptr);if(!other)return 6;selected=other;}
  EventInfo before{},after{};ULONG rb=0,ra=0;LONG qb=query(event,0,&before,8,&rb);LONG status=set(selected);LONG qa=query(event,0,&after,8,&ra);LONG repeat=set(selected);LONG clear=set(nullptr);
  if(alias&&close(alias)<0)return 7;if(other&&close(other)<0)return 8;if(close(event)<0)return 9;if(!GetProcessHandleCount(GetCurrentProcess(),&ha))return 10;
  printf("case=%u width=%zu manual=%u initial=%u mode=%u reset=%08lX status=%08lX repeat=%08lX clear=%08lX query-before=%08lX returned-before=%lu type-before=%lu state-before=%ld query-after=%08lX returned-after=%lu type-after=%lu state-after=%ld handles-before=%lu handles-after=%lu\n",ordinal++,sizeof(void *),manual,initial,mode,ULONG(reset),ULONG(status),ULONG(repeat),ULONG(clear),ULONG(qb),rb,before.type,before.state,ULONG(qa),ra,after.type,after.state,hb,ha);
  if(qb<0||qa<0||hb!=ha)return 11;
 }
 printf("complete cases=%u\n",ordinal);return 0;
}
