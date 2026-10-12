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
static uint64_t hash(const unsigned char* p,size_t n){uint64_t h=14695981039346656037ull;for(size_t i=0;i<n;i++){h^=p[i];h*=1099511628211ull;}return h;}
static void one(unsigned family,ULONG length,unsigned name,ULONG record,unsigned type,int delta,unsigned tail){
 std::vector<unsigned char> traits(65536,0);USHORT total=USHORT(length);memcpy(traits.data(),&total,2);
 if(family==0){if(name==1||name==2){memset(traits.data()+2,name==1?0x41:0xFF,length-2);traits[length-1]=0;}else if(name==3){memset(traits.data()+2,0x41,length-2);}}
 else if(family==1){USHORT n=USHORT(record);memcpy(traits.data()+3,&n,2);traits[5]=static_cast<unsigned char>(type);if(record>=19&&type==1&&tail){for(unsigned i=0;i<16;i++)traits[6+i]=static_cast<unsigned char>(i+1);}if(tail==2&&delta>=3){USHORT n2=3;memcpy(traits.data()+3+record,&n2,2);traits[5+record]=99;}}
 else {memcpy(traits.data()+2,"name",5);USHORT n=USHORT(record);memcpy(traits.data()+7,&n,2);traits[9]=static_cast<unsigned char>(type);if(record>=19&&type==1){for(unsigned i=0;i<16;i++)traits[10+i]=static_cast<unsigned char>(i+1);}}
 DWORD hb=0,ha=0;GetProcessHandleCount(GetCurrentProcess(),&hb);unsigned char reg[160]{},regout[160],input[24]{},output[120];memset(output,0xA5,120);ULONG kind=3,flag=1,ret=0;memcpy(reg+16,&kind,4);memcpy(reg+20,&flag,4);if(control(15,reg,160,regout,160,&ret)<0)fail("register");uint64_t handle=0,pointer=uintptr_t(traits.data());memcpy(&handle,regout+24,8);memcpy(input,&handle,8);memcpy(input+8,&pointer,8);memcpy(input+16,&length,4);ULONG returned=0xA5A5A5A5;LONG status=control(30,input,24,output,120,&returned);ULONG first_ret=returned;
 unsigned char valid[3]={3,0,0};pointer=uintptr_t(valid);ULONG three=3;memcpy(input+8,&pointer,8);memcpy(input+16,&three,4);returned=0xA5A5A5A5;LONG retry=control(30,input,24,output,120,&returned);LONG close=close_nt(reinterpret_cast<HANDLE>(uintptr_t(handle)));GetProcessHandleCount(GetCurrentProcess(),&ha);
 printf("case=%u width=%zu family=%u length=%lu name=%u record=%lu type=%u delta=%d tail=%u status=%08lX returned=%08lX retry=%08lX retry-returned=%08lX close=%08lX handles-before=%lu handles-after=%lu payload-hash=%llX prefix=",count++,sizeof(void*),family,length,name,record,type,delta,tail,ULONG(status),first_ret,ULONG(retry),returned,ULONG(close),hb,ha,(unsigned long long)hash(traits.data(),length));for(ULONG i=0;i<length&&i<64;i++)printf("%02X",unsigned(traits[i]));printf(" suffix=");for(ULONG i=length>64?length-64:0;i<length;i++)printf("%02X",unsigned(traits[i]));puts("");if(hb!=ha||close<0)fail("lifetime");for(auto b:output)if(b!=0xA5)fail("output-changed");
}
int main(){setvbuf(stdout,nullptr,_IONBF,0);auto n=GetModuleHandleW(L"ntdll.dll");control=reinterpret_cast<Control>(GetProcAddress(n,"NtTraceControl"));close_nt=reinterpret_cast<Close>(GetProcAddress(n,"NtClose"));if(!control||!close_nt)fail("exports");
 for(ULONG len:{3u,4u,7u,16u,32u,255u,256u,257u,65535u})for(unsigned name=0;name<4;name++)one(0,len,name,0,0,0,0);
 for(ULONG rec:{0u,1u,2u,3u,4u,18u,19u,20u,64u,65532u})for(unsigned type:{0u,1u,2u,3u,99u,255u})for(int delta:{-1,0,1,3})for(unsigned tail:{0u,1u,2u}){int length=3+int(rec)+delta;if(length>=3&&length<=65535)one(1,ULONG(length),0,rec,type,delta,tail);}
 for(ULONG rec:{3u,19u,20u})for(unsigned type:{0u,1u,99u})one(2,7+rec,1,rec,type,0,0);
 printf("complete cases=%u\n",count);return 0;
}
