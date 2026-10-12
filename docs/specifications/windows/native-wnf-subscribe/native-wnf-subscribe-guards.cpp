#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <initializer_list>
using Subscribe=LONG(NTAPI *)(const uint64_t *,ULONG,ULONG,uint64_t *);
using Unsubscribe=LONG(NTAPI *)(const uint64_t *);
int main(){setvbuf(stdout,nullptr,_IONBF,0);auto m=GetModuleHandleW(L"ntdll.dll");auto sub=reinterpret_cast<Subscribe>(GetProcAddress(m,"NtSubscribeWnfStateChange"));auto unsub=reinterpret_cast<Unsubscribe>(GetProcAddress(m,"NtUnsubscribeWnfStateChange"));SYSTEM_INFO si{};GetSystemInfo(&si);SIZE_T page=si.dwPageSize;auto mem=static_cast<unsigned char *>(VirtualAlloc(nullptr,page*3,MEM_RESERVE|MEM_COMMIT,PAGE_READWRITE));if(!mem)return 2;uint64_t known=0x41911A28A3BC10B5,invalid=0;unsigned count=0;
 for(unsigned ik=0;ik<5;ik++)for(unsigned ok=0;ok<14;ok++)for(ULONG mask:{0u,0x11u,0x20u}){DWORD old=0;VirtualProtect(mem,page*3,PAGE_READWRITE,&old);memcpy(mem,&known,8);uint64_t poison=0xA5A5A5A5A5A5A5A5;memcpy(mem+page,&poison,8);if(ok>=5)memcpy(mem+page*2-(ok<8?4:ok<11?1:3),&poison,8);uint64_t local=poison;const uint64_t *input=ik==0?&known:ik==1||ik==2?reinterpret_cast<uint64_t *>(mem):ik==3?nullptr:&invalid;uint64_t *output=ok==0?&local:ok==3?nullptr:reinterpret_cast<uint64_t *>(mem+(ok>=5?page*2-(ok<8?4:ok<11?1:3):page));if(ik==1||ik==2)VirtualProtect(mem,page,ik==1?PAGE_READWRITE|PAGE_GUARD:PAGE_NOACCESS,&old);if(ok==1||ok==2||ok==4)VirtualProtect(mem+page,page,ok==1?PAGE_READWRITE|PAGE_GUARD:ok==2?PAGE_NOACCESS:PAGE_READONLY,&old);if(ok>=5)VirtualProtect(mem+page*2,page,(ok-5)%3==0?PAGE_READONLY:(ok-5)%3==1?PAGE_READWRITE|PAGE_GUARD:PAGE_NOACCESS,&old);unsub(&known);LONG status=sub(input,0,mask,output);MEMORY_BASIC_INFORMATION a{},b{},d{};VirtualQuery(mem,&a,sizeof a);VirtualQuery(mem+page,&b,sizeof b);VirtualQuery(mem+page*2,&d,sizeof d);VirtualProtect(mem,page*3,PAGE_READWRITE,&old);uint64_t after=local;if(ok!=0&&ok!=3)memcpy(&after,output,8);LONG end=unsub(&known);printf("case=%u width=%zu input=%u output=%u mask=%08lX status=%08lX input-protect=%08lX output-protect=%08lX upper-protect=%08lX after=%016llX end=%08lX\n",count++,sizeof(void *),ik,ok,mask,ULONG(status),a.Protect,b.Protect,d.Protect,(unsigned long long)after,ULONG(end));}
 VirtualFree(mem,0,MEM_RELEASE);printf("complete cases=%u\n",count);return 0;}
