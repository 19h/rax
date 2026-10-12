#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <initializer_list>
using Control=LONG(NTAPI*)(ULONG,void*,ULONG,void*,ULONG,ULONG*);using Close=LONG(NTAPI*)(HANDLE);
static void dump(const unsigned char* b,size_t n){for(size_t i=0;i<n;i++)printf("%02X",unsigned(b[i]));}
int main(){setvbuf(stdout,nullptr,_IONBF,0);auto n=GetModuleHandleW(L"ntdll.dll");auto control=reinterpret_cast<Control>(GetProcAddress(n,"NtTraceControl"));auto close_nt=reinterpret_cast<Close>(GetProcAddress(n,"NtClose"));if(!control||!close_nt)return 2;unsigned count=0;
 for(unsigned rb=0;rb<2;rb++)for(int rd:{-4,-1,0,1,2,3,7,8,15,16,20,23,24,119,120,256})for(int od:{-80,-3,-2,-1,0,1,2,3,80})for(ULONG length:{3u,7u,32u}){
  DWORD hb=0,ha=0;GetProcessHandleCount(GetCurrentProcess(),&hb);unsigned char reg[160]{},regout[160],bytes[1024],original[1024],retry[24],retry_output[120];memset(bytes,0xA5,1024);ULONG kind=3,flag=1,regret=0;memcpy(reg+16,&kind,4);memcpy(reg+20,&flag,4);if(control(15,reg,160,regout,160,&regret)<0)return 3;uint64_t own=0,pointer=uintptr_t(bytes+256);memcpy(&own,regout+24,8);memset(bytes+512,0,24);memcpy(bytes+512,&own,8);memcpy(bytes+520,&pointer,8);memcpy(bytes+528,&length,4);memcpy(retry,bytes+512,24);USHORT len=USHORT(length);memcpy(bytes+256,&len,2);memset(bytes+258,0x41,length-2);bytes[255+length]=0;memcpy(original,bytes,1024);
  ULONG* returned=reinterpret_cast<ULONG*>(bytes+(rb?256:512)+rd);LONG status=control(30,bytes+512,24,bytes+512+od,120,returned);ULONG value=0;memcpy(&value,returned,4);ULONG retry_ret=0xA5A5A5A5;memset(retry_output,0xA5,120);LONG repeated=control(30,retry,24,retry_output,120,&retry_ret);LONG close=close_nt(reinterpret_cast<HANDLE>(uintptr_t(own)));GetProcessHandleCount(GetCurrentProcess(),&ha);
  printf("case=%u width=%zu returned-base=%u returned-delta=%d output-delta=%d length=%lu status=%08lX returned=%08lX retry=%08lX retry-returned=%08lX close=%08lX handles-before=%lu handles-after=%lu before=",count++,sizeof(void*),rb,rd,od,length,ULONG(status),value,ULONG(repeated),retry_ret,ULONG(close),hb,ha);dump(original+224,576);printf(" after=");dump(bytes+224,576);puts("");if(hb!=ha||close<0)return 4;
 }
 printf("complete cases=%u\n",count);return 0;
}
