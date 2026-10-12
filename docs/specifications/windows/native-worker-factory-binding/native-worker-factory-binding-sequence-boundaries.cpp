#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <winternl.h>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <initializer_list>
using Create=LONG(NTAPI *)(PHANDLE,ACCESS_MASK,POBJECT_ATTRIBUTES,HANDLE,HANDLE,PVOID,PVOID,ULONG,SIZE_T,SIZE_T);
using Port=LONG(NTAPI *)(PHANDLE,ACCESS_MASK,POBJECT_ATTRIBUTES,ULONG);
using Set=LONG(NTAPI *)(HANDLE,ULONG,PVOID,ULONG);
using Query=LONG(NTAPI *)(HANDLE,ULONG,PVOID,ULONG,PULONG);
using Close=LONG(NTAPI *)(HANDLE);
static VOID NTAPI worker(PVOID) {}
static ULONG field(const unsigned char *p,unsigned at){ULONG v;memcpy(&v,p+at,4);return v;}
int main(){
 setvbuf(stdout,nullptr,_IONBF,0);auto n=GetModuleHandleW(L"ntdll.dll");
 auto create=reinterpret_cast<Create>(GetProcAddress(n,"NtCreateWorkerFactory"));
 auto port_create=reinterpret_cast<Port>(GetProcAddress(n,"NtCreateIoCompletion"));
 auto set=reinterpret_cast<Set>(GetProcAddress(n,"NtSetInformationWorkerFactory"));
 auto query=reinterpret_cast<Query>(GetProcAddress(n,"NtQueryInformationWorkerFactory"));
 auto close=reinterpret_cast<Close>(GetProcAddress(n,"NtClose"));
 if(!create||!port_create||!set||!query||!close)return 2;
 DWORD hb=0,ha=0;if(!GetProcessHandleCount(GetCurrentProcess(),&hb))return 3;
 HANDLE port=nullptr,factory=nullptr;if(port_create(&port,0x1F0003,nullptr,0)<0||create(&factory,0xF00FF,nullptr,port,GetCurrentProcess(),reinterpret_cast<void *>(&worker),nullptr,512,0,0)<0)return 4;
 unsigned step=0;
 for(ULONG value:{0xFFFFFFFFul,0x00000001ul,0xFFFFFFFFul,0x00000002ul,0xFFFFFFFDul,0x7FFFFFFFul,0x00000001ul,0x7FFFFFFFul,0x00000001ul,0xFFFFFFFFul,0x80000000ul,0x80000000ul,0x00000004ul,0x00000000ul,0xFFFFFFFFul,0x00000002ul,0x80000000ul,0x80000000ul,0x80000000ul,0x7FFFFFFFul,0x7FFFFFFFul,0x00000002ul,0x00000000ul,0x00000001ul,0xFFFFFFFFul,0x00000000ul}){
  unsigned char before[128],after[128];memset(before,0xA5,128);memset(after,0xA5,128);ULONG rb=0,ra=0;ULONG size=sizeof(void *)==8?120:96;
  LONG qb=query(factory,7,before,size,&rb);LONG status=set(factory,3,&value,4);LONG qa=query(factory,7,after,size,&ra);
  printf("step=%u width=%zu class=3 value=%08lX status=%08lX query-before=%08lX returned-before=%lu binding-before=%08lX maximum-before=%08lX total-before=%lu query-after=%08lX returned-after=%lu binding-after=%08lX maximum-after=%08lX total-after=%lu before=",step++,sizeof(void *),value,ULONG(status),ULONG(qb),rb,field(before,32),field(before,40),field(before,52),ULONG(qa),ra,field(after,32),field(after,40),field(after,52));
  for(auto v:before)printf("%02X",v);printf(" after=");for(auto v:after)printf("%02X",v);puts("");
  if(qb<0||qa<0||field(before,52)||field(after,52))return 5;
 }
 if(close(factory)<0||close(port)<0||!GetProcessHandleCount(GetCurrentProcess(),&ha))return 6;
 printf("complete steps=%u handles-before=%lu handles-after=%lu\n",step,hb,ha);return hb==ha?0:7;
}
