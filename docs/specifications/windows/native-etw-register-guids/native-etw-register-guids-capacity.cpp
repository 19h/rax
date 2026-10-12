#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <vector>
using Control=LONG(NTAPI*)(ULONG,void*,ULONG,void*,ULONG,ULONG*);using Close=LONG(NTAPI*)(HANDLE);
int main(){setvbuf(stdout,nullptr,_IONBF,0);auto n=GetModuleHandleW(L"ntdll.dll");auto control=reinterpret_cast<Control>(GetProcAddress(n,"NtTraceControl"));auto close_nt=reinterpret_cast<Close>(GetProcAddress(n,"NtClose"));if(!control||!close_nt)return 2;unsigned total=0;
 for(unsigned unique=0;unique<2;unique++){
  DWORD before=0,after=0;if(!GetProcessHandleCount(GetCurrentProcess(),&before))return 3;std::vector<HANDLE>handles;
  for(unsigned ordinal=0;ordinal<1030;ordinal++){
   unsigned char input[160]{},output[160];memset(output,0xA5,160);const unsigned char guid[16]={0x21,0xDC,0xBC,0xD4,0x9A,0x92,0xC1,0x43,0x8D,0x2D,0x5B,0x75,0x38,0xA6,0x62,0xF1};memcpy(input,guid,16);if(unique)memcpy(input,&ordinal,4);ULONG kind=3,flag=1,returned[2]={0xA5A5A5A5,0xB6B6B6B6};memcpy(input+16,&kind,4);memcpy(input+20,&flag,4);LONG status=control(15,input,160,output,160,returned);uint64_t h=0;memcpy(&h,output+24,8);DWORD live=0;if(!GetProcessHandleCount(GetCurrentProcess(),&live))return 4;if(status>=0){if(!h||h>UINTPTR_MAX)return 5;handles.push_back(reinterpret_cast<HANDLE>(uintptr_t(h)));}
   printf("case=%u width=%zu unique=%u ordinal=%u status=%08lX returned=%08lX canary=%08lX handle=%llX live=%lu baseline=%lu successful=%zu\n",total++,sizeof(void*),unique,ordinal,ULONG(status),returned[0],returned[1],(unsigned long long)h,live,before,handles.size());if(returned[1]!=0xB6B6B6B6)return 6;
  }
  for(unsigned parity=0;parity<2;parity++)for(size_t i=parity;i<handles.size();i+=2)if(close_nt(handles[i])<0)return 7;if(!GetProcessHandleCount(GetCurrentProcess(),&after)||after!=before)return 8;printf("drain unique=%u handles-before=%lu handles-after=%lu registered=%zu\n",unique,before,after,handles.size());
 }
 printf("complete cases=%u\n",total);return 0;
}
