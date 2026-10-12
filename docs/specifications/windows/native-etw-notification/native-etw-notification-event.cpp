#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <initializer_list>
using Control=LONG(NTAPI *)(ULONG,void *,ULONG,void *,ULONG,ULONG *);
using Query=LONG(NTAPI *)(HANDLE,ULONG,void *,ULONG,ULONG *);
struct EventInfo{ULONG type;LONG state;};
int main(){
 setvbuf(stdout,nullptr,_IONBF,0);auto n=GetModuleHandleW(L"ntdll.dll");
 auto control=reinterpret_cast<Control>(GetProcAddress(n,"NtTraceControl"));
 auto query=reinterpret_cast<Query>(GetProcAddress(n,"NtQueryEvent"));
 if(!control||!query)return 2;unsigned ordinal=0;
 for(unsigned manual:{0u,1u})for(unsigned initial:{0u,1u})for(unsigned mode=0;mode<12;mode++)for(ULONG length:{0u,1u,2u,3u,4u,5u,7u,8u,9u,16u}){
  DWORD hb=0,ha=0;if(!GetProcessHandleCount(GetCurrentProcess(),&hb))return 3;
  HANDLE event=CreateEventW(nullptr,manual,initial,nullptr),alias=nullptr,other=nullptr;if(!event)return 4;HANDLE selected=event;
  if(mode==1)selected=nullptr;else if(mode==2)selected=reinterpret_cast<HANDLE>(uintptr_t(0x13572468));
  else if(mode==3)selected=GetCurrentProcess();else if(mode==4)selected=GetCurrentThread();
  else if(mode>=5&&mode<=10){ULONG grant[]={0,1,2,3,0x100000,0x1F0003};if(!DuplicateHandle(GetCurrentProcess(),event,GetCurrentProcess(),&alias,grant[mode-5],FALSE,0))return 5;selected=alias;}
  else if(mode==11){other=CreateSemaphoreW(nullptr,0,1,nullptr);if(!other)return 6;selected=other;}
  unsigned char input[24]{};uint64_t encoded=uint64_t(uintptr_t(selected));memcpy(input,&encoded,8);
  ULONG returned[2]={0xA5A5A5A5,0xB6B6B6B6};EventInfo before{},after{};ULONG rb=0,ra=0;
  LONG qb=query(event,0,&before,8,&rb),status=control(27,input,length,nullptr,0,returned);
  ULONG first_returned=returned[0];returned[0]=0xA5A5A5A5;
  LONG repeat=control(27,input,length,nullptr,0,returned),qa=query(event,0,&after,8,&ra);
  if(alias&&!CloseHandle(alias))return 7;if(other&&!CloseHandle(other))return 8;if(!CloseHandle(event))return 9;
  if(!GetProcessHandleCount(GetCurrentProcess(),&ha))return 10;
  printf("case=%u width=%zu manual=%u initial=%u mode=%u length=%lu status=%08lX returned=%08lX repeat=%08lX repeat-returned=%08lX canary=%08lX query-before=%08lX returned-before=%lu type-before=%lu state-before=%ld query-after=%08lX returned-after=%lu type-after=%lu state-after=%ld handles-before=%lu handles-after=%lu\n",ordinal++,sizeof(void *),manual,initial,mode,length,ULONG(status),first_returned,ULONG(repeat),returned[0],returned[1],ULONG(qb),rb,before.type,before.state,ULONG(qa),ra,after.type,after.state,hb,ha);
  if(qb<0||qa<0||hb!=ha||returned[1]!=0xB6B6B6B6)return 11;
 }
 printf("complete cases=%u\n",ordinal);return 0;
}
