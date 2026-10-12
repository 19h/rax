#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <initializer_list>
using Control=LONG(NTAPI *)(ULONG,void *,ULONG,void *,ULONG,ULONG *);
int main(){
 setvbuf(stdout,nullptr,_IONBF,0);auto n=GetModuleHandleW(L"ntdll.dll");auto control=reinterpret_cast<Control>(GetProcAddress(n,"NtTraceControl"));if(!control)return 2;
 SYSTEM_INFO si{};GetSystemInfo(&si);SIZE_T page=si.dwPageSize;
 auto memory=static_cast<unsigned char *>(VirtualAlloc(nullptr,5*page,MEM_RESERVE|MEM_COMMIT,PAGE_READWRITE));auto close_nt=reinterpret_cast<LONG(NTAPI *)(HANDLE)>(GetProcAddress(n,"NtClose"));if(!memory||!close_nt)return 3;
 unsigned char header[160]{};const unsigned char guid[16]={0x21,0xDC,0xBC,0xD4,0x9A,0x92,0xC1,0x43,0x8D,0x2D,0x5B,0x75,0x38,0xA6,0x62,0xF1};memcpy(header,guid,16);ULONG kind=3,flag=1;memcpy(header+16,&kind,4);memcpy(header+20,&flag,4);unsigned count=0;
 for(unsigned ik=0;ik<10;ik++)for(unsigned rk=0;rk<7;rk++)for(unsigned ok=0;ok<5;ok++)for(ULONG il:{0u,159u,160u,161u})for(ULONG ol:{0u,159u,160u,0xFFFFFFFFu}){
  DWORD old=0;if(!VirtualProtect(memory,5*page,PAGE_READWRITE,&old))return 4;memset(memory,0xA5,5*page);
  unsigned char local_input[192],local_return[16],local_output[192];memset(local_input,0xA5,192);memset(local_return,0xA5,16);memset(local_output,0xA5,192);
  unsigned input_offset=ik>=7?ik-6:0;memcpy(local_input+input_offset,header,160);memcpy(memory,header,160);memcpy(memory+page-80,header,160);
  void *input=ik==0||ik>=7?local_input+input_offset:ik==1?nullptr:ik==2?reinterpret_cast<void *>(uintptr_t(1)):ik==6?memory+page-80:memory;
  ULONG *returned=rk==0?reinterpret_cast<ULONG *>(local_return):rk==1?nullptr:rk==2?reinterpret_cast<ULONG *>(uintptr_t(1)):rk==6?reinterpret_cast<ULONG *>(memory+3*page-2):reinterpret_cast<ULONG *>(memory+2*page);
  void *output=ok==0?nullptr:ok==1?local_output:memory+4*page;
  if(ik>=3&&ik<=5&&!VirtualProtect(memory,page,ik==3?PAGE_READWRITE|PAGE_GUARD:ik==4?PAGE_NOACCESS:PAGE_READONLY,&old))return 5;
  if(ik==6&&!VirtualProtect(memory+page,page,PAGE_NOACCESS,&old))return 6;
  if(rk>=3&&rk<=5&&!VirtualProtect(memory+2*page,page,rk==3?PAGE_READWRITE|PAGE_GUARD:rk==4?PAGE_NOACCESS:PAGE_READONLY,&old))return 7;
  if(rk==6&&!VirtualProtect(memory+3*page,page,PAGE_NOACCESS,&old))return 8;
  if(ok>=2&&!VirtualProtect(memory+4*page,page,ok==2?PAGE_NOACCESS:ok==3?PAGE_READWRITE|PAGE_GUARD:PAGE_READONLY,&old))return 9;
  LONG status=control(15,input,il,output,ol,returned);
  MEMORY_BASIC_INFORMATION in{},ret{},out{},cross{};VirtualQuery(memory,&in,sizeof in);VirtualQuery(memory+2*page,&ret,sizeof ret);VirtualQuery(memory+4*page,&out,sizeof out);VirtualQuery(memory+3*page,&cross,sizeof cross);
  VirtualProtect(memory,5*page,PAGE_READWRITE,&old);uint64_t handle=0;LONG closed=LONG(0xDEADBEEF);if(status>=0){if(!output)return 10;memcpy(&handle,static_cast<unsigned char *>(output)+24,8);closed=close_nt(reinterpret_cast<HANDLE>(uintptr_t(handle)));if(closed<0)return 11;}ULONG after=0xA5A5A5A5;if(rk!=1&&rk!=2)memcpy(&after,returned,4);ULONG output_after=0xA5A5A5A5;if(ok!=0)memcpy(&output_after,output,4);
  printf("case=%u width=%zu input=%u returned=%u output=%u input-length=%lu output-length=%lu status=%08lX return-after=%08lX output-after=%08lX input-protect=%08lX returned-protect=%08lX returned-upper-protect=%08lX output-protect=%08lX handle=%llX close=%08lX\n",count++,sizeof(void *),ik,rk,ok,il,ol,ULONG(status),after,output_after,in.Protect,ret.Protect,cross.Protect,out.Protect,(unsigned long long)handle,ULONG(closed));
 }
 VirtualFree(memory,0,MEM_RELEASE);printf("complete cases=%u\n",count);return 0;
}
