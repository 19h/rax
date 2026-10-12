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
 for(uint64_t selected:{0x02821B2CA3BC08B5ull,0x0D920D2EA3BC08B5ull,0x41911A28A3BC10B5ull,0x41960A28A3BC50B5ull,0x41C61629A3BC30B5ull,0x41C61629A3BC60B5ull,0x41C61629A3BCA8B5ull,0x41C61629A3BD10B5ull,0x41C61D3DA3BC10B5ull}){known=selected;for(ULONG mask:{0u,1u,2u,4u,8u,0x10u,0x11u,0x1Fu}){unsub(&known);unsigned char bytes[64];memset(bytes,0xA5,64);ULONG stamp=0xA5A5A5A5,size=64;LONG before=query(&known,nullptr,nullptr,&stamp,bytes,&size);printf("data-before name=%016llX mask=%08lX status=%08lX stamp=%08lX size=%lu\n",(unsigned long long)known,mask,ULONG(before),stamp,size);uint64_t id=0;LONG status=sub(&known,0,mask,&id);stamp=0xA5A5A5A5;size=64;LONG after=query(&known,nullptr,nullptr,&stamp,bytes,&size);printf("data-after name=%016llX mask=%08lX subscribed=%08lX status=%08lX stamp=%08lX size=%lu\n",(unsigned long long)known,mask,ULONG(status),ULONG(after),stamp,size);unsub(&known);}
 }printf("complete names=9 data-pairs=72\n");return 0;}
