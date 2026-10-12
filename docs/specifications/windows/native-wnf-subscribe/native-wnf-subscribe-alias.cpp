#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <initializer_list>
using Subscribe=LONG(NTAPI *)(const uint64_t *,ULONG,ULONG,uint64_t *);
using Unsubscribe=LONG(NTAPI *)(const uint64_t *);
using Query=LONG(NTAPI *)(const uint64_t *,void *,void *,ULONG *,void *,ULONG *);
int main(){setvbuf(stdout,nullptr,_IONBF,0);auto m=GetModuleHandleW(L"ntdll.dll");auto sub=reinterpret_cast<Subscribe>(GetProcAddress(m,"NtSubscribeWnfStateChange"));auto unsub=reinterpret_cast<Unsubscribe>(GetProcAddress(m,"NtUnsubscribeWnfStateChange"));auto query=reinterpret_cast<Query>(GetProcAddress(m,"NtQueryWnfStateData"));uint64_t known=0x41911A28A3BC10B5;unsigned ordinal=0;
 for(int delta=-9;delta<=9;delta++)for(ULONG mask:{0u,0x11u,0x20u}){unsub(&known);unsigned char memory[64];memset(memory,0xA5,64);memcpy(memory+24,&known,8);uint64_t id=0;auto name=reinterpret_cast<uint64_t *>(memory+24),out=reinterpret_cast<uint64_t *>(memory+24+delta);LONG status=sub(name,0,mask,out);memcpy(&id,out,8);uint64_t resulting=0;memcpy(&resulting,name,8);LONG end=unsub(&known);printf("alias-case=%u width=%zu delta=%d mask=%08lX status=%08lX id=%016llX name-after=%016llX end=%08lX\n",ordinal++,sizeof(void *),delta,mask,ULONG(status),(unsigned long long)id,(unsigned long long)resulting,ULONG(end));}
 for(ULONG mask:{0u,1u,2u,4u,8u,0x10u,0x11u,0x1Fu}){unsub(&known);unsigned char bytes[64];memset(bytes,0xA5,64);ULONG stamp=0xA5A5A5A5,size=64;LONG before=query(&known,nullptr,nullptr,&stamp,bytes,&size);printf("data-before mask=%08lX status=%08lX stamp=%08lX size=%lu\n",mask,ULONG(before),stamp,size);uint64_t id=0;LONG status=sub(&known,0,mask,&id);stamp=0xA5A5A5A5;size=64;LONG after=query(&known,nullptr,nullptr,&stamp,bytes,&size);printf("data-after mask=%08lX subscribed=%08lX status=%08lX stamp=%08lX size=%lu\n",mask,ULONG(status),ULONG(after),stamp,size);unsub(&known);}
 printf("complete alias-cases=%u data-pairs=8\n",ordinal);return 0;}
