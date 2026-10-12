#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <vector>
#include <initializer_list>
using Control=LONG(NTAPI*)(ULONG,void*,ULONG,void*,ULONG,ULONG*);using Close=LONG(NTAPI*)(HANDLE);
static Control control;static Close close_nt;static unsigned count=0;
static void fail(const char* s){printf("failure=%s error=%lu\n",s,GetLastError());ExitProcess(21);}
static void dump(const unsigned char* b,size_t n){for(size_t i=0;i<n;i++)printf("%02X",unsigned(b[i]));}
static void one(unsigned family,ULONG il,ULONG ol,unsigned mode,ULONG length,ULONG embedded,unsigned name,unsigned handle_mode,uint64_t high){
 DWORD hb=0,ha=0;if(!GetProcessHandleCount(GetCurrentProcess(),&hb))fail("count-before");
 unsigned char registration[160]{},provider[160],input[256]{},output[256],original[256];ULONG k=3,f=1,returned=0;memcpy(registration+16,&k,4);memcpy(registration+20,&f,4);
 LONG setup=control(15,registration,160,provider,160,&returned);uint64_t actual=0;memcpy(&actual,provider+24,8);if(setup<0||!actual)fail("registration");
 std::vector<unsigned char> traits(65536,0);if(name==2)memset(traits.data()+2,0x41,traits.size()-2);USHORT size=USHORT(embedded);memcpy(traits.data(),&size,2);
 if(name==1)memcpy(traits.data()+2,"name",5);else if(name==3){memcpy(traits.data()+2,"name",5);USHORT record=3;memcpy(traits.data()+7,&record,2);traits[9]=99;}
 uint64_t h=actual;HANDLE event=nullptr;
 if(handle_mode==1)h=0;else if(handle_mode==2)h=1;else if(handle_mode==3){if(close_nt(reinterpret_cast<HANDLE>(uintptr_t(actual)))<0)fail("close-before");h=actual;}else if(handle_mode==4){event=CreateEventW(nullptr,FALSE,FALSE,nullptr);if(!event)fail("event");h=uintptr_t(event);}else if(handle_mode==5)h|=high;else if(handle_mode==6)h=0xFFFFFFFC;
 uint64_t pointer=uintptr_t(traits.data()),len=uint64_t(length)|(high<<32);memcpy(input,&h,8);memcpy(input+8,&pointer,8);memcpy(input+16,&len,8);memcpy(original,input,sizeof(input));memset(output,0xA5,sizeof(output));ULONG ret[2]={0xA5A5A5A5,0xB6B6B6B6};void* in=mode==1?nullptr:input;void* out=mode==2?nullptr:mode==3?input:output;ULONG* rp=mode==4?nullptr:ret;
 LONG first=control(30,in,il,out,ol,rp);ULONG first_ret=ret[0];LONG repeat=control(30,in,il,out,ol,rp);ULONG second_ret=ret[0];LONG close=LONG(0xDEADBEEF);if(handle_mode!=3)close=close_nt(reinterpret_cast<HANDLE>(uintptr_t(actual)));if(event)CloseHandle(event);if(!GetProcessHandleCount(GetCurrentProcess(),&ha))fail("count-after");
 printf("case=%u width=%zu family=%u input-length=%lu output-length=%lu mode=%u length=%lu embedded=%lu name=%u handle-mode=%u high=%llX status=%08lX returned=%08lX repeat=%08lX repeat-returned=%08lX canary=%08lX close=%08lX handles-before=%lu handles-after=%lu input-before=",count++,sizeof(void*),family,il,ol,mode,length,embedded,name,handle_mode,(unsigned long long)high,ULONG(first),first_ret,ULONG(repeat),second_ret,ret[1],ULONG(close),hb,ha);dump(original,32);printf(" input-after=");dump(input,32);printf(" output=");dump(output,160);puts("");if(hb!=ha||ret[1]!=0xB6B6B6B6)fail("lifetime-canary");
}
int main(){setvbuf(stdout,nullptr,_IONBF,0);auto n=GetModuleHandleW(L"ntdll.dll");control=reinterpret_cast<Control>(GetProcAddress(n,"NtTraceControl"));close_nt=reinterpret_cast<Close>(GetProcAddress(n,"NtClose"));if(!control||!close_nt)fail("exports");
 for(ULONG il:{0u,1u,4u,16u,23u,24u,25u,32u,48u,160u})for(ULONG ol:{0u,1u,4u,119u,120u,121u,160u,256u})one(0,il,ol,0,7,7,1,0,0);
 for(unsigned mode:{1u,2u,3u,4u})for(ULONG ol:{0u,119u,120u,160u})one(1,24,ol,mode,7,7,1,0,0);
 for(ULONG length:{0u,1u,2u,3u,4u,5u,7u,16u,31u,32u,33u,255u,256u,257u,65534u,65535u,65536u})for(ULONG embedded:{0u,1u,2u,3u,4u,5u,7u,16u,32u,255u,256u,257u,65535u})for(unsigned name=0;name<4;name++)one(2,24,120,0,length,embedded,name,0,0);
 for(unsigned h=0;h<7;h++)for(ULONG length:{0u,1u,2u,3u,7u,65535u,65536u})one(3,24,120,0,length,length,0,h,h==5?uint64_t(0x100000000):0);
 for(uint64_t high:{uint64_t(1),uint64_t(0x13572468),uint64_t(0xFFFFFFFF)})one(4,24,120,0,7,7,1,0,high);
 printf("complete cases=%u\n",count);return 0;
}
