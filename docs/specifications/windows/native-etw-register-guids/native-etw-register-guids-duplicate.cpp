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
static std::vector<uintptr_t> handles(){std::vector<uintptr_t>v;for(uintptr_t h=4;h<65536;h+=4){DWORD f=0;if(GetHandleInformation(reinterpret_cast<HANDLE>(h),&f))v.push_back(h);}return v;}
static std::vector<uintptr_t> added(const std::vector<uintptr_t>& before){std::vector<uintptr_t>v;for(auto h:handles()){bool seen=false;for(auto b:before)if(b==h)seen=true;if(!seen)v.push_back(h);}return v;}
static void header(unsigned char* b){memset(b,0,160);const unsigned char guid[16]={0x21,0xDC,0xBC,0xD4,0x9A,0x92,0xC1,0x43,0x8D,0x2D,0x5B,0x75,0x38,0xA6,0x62,0xF1};memcpy(b,guid,16);ULONG kind=3,flag=1;memcpy(b+16,&kind,4);memcpy(b+20,&flag,4);for(unsigned i=48;i<160;i++)b[i]=static_cast<unsigned char>(i);}
static void dump(const unsigned char* b,size_t n){for(size_t i=0;i<n;i++)printf("%02X",unsigned(b[i]));}
static BOOL duplicate_call(HANDLE h,HANDLE target,HANDLE* out,DWORD grant,DWORD options,DWORD* exception){*exception=0;__try{return DuplicateHandle(GetCurrentProcess(),h,target,out,grant,FALSE,options);}__except(EXCEPTION_EXECUTE_HANDLER){*exception=GetExceptionCode();return FALSE;}}
int main(){setvbuf(stdout,nullptr,_IONBF,0);auto n=GetModuleHandleW(L"ntdll.dll");control=reinterpret_cast<Control>(GetProcAddress(n,"NtTraceControl"));close_nt=reinterpret_cast<Close>(GetProcAddress(n,"NtClose"));if(!control||!close_nt)fail("exports");
 for(DWORD options:{0u,1u,2u,3u,4u,5u,6u,7u,8u,16u,32u,0xFFFFFFFFu})for(DWORD grant:{0u,4u,0x804u,0xFFFFFFFFu})for(unsigned output_mode:{0u,1u,2u})for(unsigned target_mode:{0u,1u}){
  auto before=handles();unsigned char in[160],out[160];header(in);ULONG returned=0;LONG s=control(15,in,160,out,160,&returned);uint64_t h=0;memcpy(&h,out+24,8);if(s<0||added(before).size()!=1)fail("duplicate-setup");HANDLE output=reinterpret_cast<HANDLE>(uintptr_t(0xA5A5A5A5));HANDLE* address=output_mode==0?&output:output_mode==1?nullptr:reinterpret_cast<HANDLE*>(uintptr_t(1));DWORD exception=0;SetLastError(0xB6B6B6B6);BOOL ok=duplicate_call(reinterpret_cast<HANDLE>(uintptr_t(h)),target_mode?nullptr:GetCurrentProcess(),address,grant,options,&exception);DWORD error=GetLastError(),flags=0;BOOL live=GetHandleInformation(reinterpret_cast<HANDLE>(uintptr_t(h)),&flags);LONG close=LONG(0xDEADBEEF);if(live)close=close_nt(reinterpret_cast<HANDLE>(uintptr_t(h)));auto remaining=added(before);for(auto x:remaining)if(close_nt(reinterpret_cast<HANDLE>(x))<0)fail("duplicate-extra-drain");
  printf("duplicate case=%u width=%zu options=%lu grant=%08lX output-mode=%u target-mode=%u ok=%u error=%08lX exception=%08lX output=%llX source-live=%u flags=%lu close=%08lX extras=%zu restored=%u\n",count++,sizeof(void*),options,grant,output_mode,target_mode,ok,error,exception,(unsigned long long)uintptr_t(output),live,flags,ULONG(close),remaining.size(),handles()==before);if(handles()!=before)fail("duplicate-restoration");
 }
 printf("duplicate-complete cases=%u\n",count);return 0;
}
