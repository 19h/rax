#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <cstdio>
#include <initializer_list>
int main(){setvbuf(stdout,nullptr,_IONBF,0);auto f=reinterpret_cast<ULONG(NTAPI *)(LONG)>(GetProcAddress(GetModuleHandleW(L"ntdll.dll"),"RtlNtStatusToDosError"));if(!f)return 2;unsigned i=0;for(ULONG status:{0u,0xC0000008u,0xC0000024u,0xC0000022u,0xC0000718u})printf("case=%u width=%zu status=%08lX error=%lu\n",i++,sizeof(void *),status,f(LONG(status)));puts("complete cases=5");return 0;}
