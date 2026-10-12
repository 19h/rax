#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <cstdint>
#include <cstdio>
using Subscribe=LONG(NTAPI *)(const uint64_t *,ULONG,ULONG,uint64_t *);
using Unsubscribe=LONG(NTAPI *)(const uint64_t *);
int main(){setvbuf(stdout,nullptr,_IONBF,0);auto m=GetModuleHandleW(L"ntdll.dll");auto sub=reinterpret_cast<Subscribe>(GetProcAddress(m,"NtSubscribeWnfStateChange"));auto unsub=reinterpret_cast<Unsubscribe>(GetProcAddress(m,"NtUnsubscribeWnfStateChange"));unsigned ordinal=0;
 for(unsigned version=0;version<16;version++)for(unsigned lifetime=0;lifetime<4;lifetime++)for(unsigned scope=0;scope<16;scope++)for(unsigned permanent=0;permanent<2;permanent++){uint64_t name=((0x41911A28A3BC10B5ull^0x41C64E6DA3BC0074ull)&~uint64_t(0x7FF))|version|(lifetime<<4)|(scope<<6)|(permanent<<10);name^=0x41C64E6DA3BC0074ull;unsub(&name);uint64_t id=~uint64_t(0);LONG status=sub(&name,0,0x11,&id),end=unsub(&name);printf("case=%u width=%zu version=%u lifetime=%u scope=%u permanent=%u name=%016llX status=%08lX id=%016llX end=%08lX\n",ordinal++,sizeof(void *),version,lifetime,scope,permanent,(unsigned long long)name,ULONG(status),(unsigned long long)id,ULONG(end));}
 printf("complete cases=%u\n",ordinal);return 0;}
