#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <winternl.h>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <initializer_list>
using Control=LONG(NTAPI*)(ULONG,void*,ULONG,void*,ULONG,ULONG*);
using Query=LONG(NTAPI*)(HANDLE,ULONG,void*,ULONG,ULONG*);
using Close=LONG(NTAPI*)(HANDLE);
using Wait=LONG(NTAPI*)(HANDLE,BOOLEAN,LARGE_INTEGER*);
static Control control;static Query query;static Close close_nt;static Wait wait_nt;static unsigned ordinal=0;
static void fail(const char* what){printf("failure=%s error=%lu\n",what,GetLastError());ExitProcess(21);}
static void dump(const unsigned char* bytes,size_t size){for(size_t i=0;i<size;i++)printf("%02X",unsigned(bytes[i]));}
static void one(unsigned family,unsigned offset,uint64_t value,ULONG in_len,ULONG out_len,unsigned mode){
 DWORD hb=0,ha=0;if(!GetProcessHandleCount(GetCurrentProcess(),&hb))fail("count-before");
 alignas(16) unsigned char input[512]{},output[512],before[512];memset(output,0xA5,sizeof(output));
 const unsigned char guid[16]={0x21,0xDC,0xBC,0xD4,0x9A,0x92,0xC1,0x43,0x8D,0x2D,0x5B,0x75,0x38,0xA6,0x62,0xF1};memcpy(input,guid,16);ULONG kind=3,flag=1;memcpy(input+16,&kind,4);memcpy(input+20,&flag,4);
 if(family==1)memcpy(input+offset,&value,4);else if(family==2)memcpy(input+offset,&value,8);else if(family==3)memset(input+offset,static_cast<unsigned char>(value),4);else if(family==4)memset(input,0,16);
 memcpy(before,input,sizeof(input));ULONG returned[2]={0xA5A5A5A5,0xB6B6B6B6};void* in=mode==1?nullptr:input;void* out=mode==2?nullptr:mode==3?input:output;ULONG* ret=mode==4?nullptr:returned;
 LONG status=control(15,in,in_len,out,out_len,ret);uint64_t handle=0;memcpy(&handle,static_cast<unsigned char*>(out?out:output)+24,8);LONG qb=LONG(0xDEADBEEF),qt=qb,wait=qb,dup_status=qb,close=qb,alias_q=qb,alias_close=qb;ULONG grant=0,hc=0,pc=0,br=0,tr=0;unsigned char basic[56]{},type[512]{};char type_name[128]{};HANDLE alias=nullptr;
 if(status>=0){if(!out||handle==0||handle>UINTPTR_MAX)fail("success-handle-shape");HANDLE h=reinterpret_cast<HANDLE>(uintptr_t(handle));qb=query(h,0,basic,sizeof(basic),&br);memcpy(&grant,basic+4,4);memcpy(&hc,basic+8,4);memcpy(&pc,basic+12,4);qt=query(h,2,type,sizeof(type),&tr);if(qt>=0){auto s=reinterpret_cast<UNICODE_STRING*>(type);if(s->Length/2>=sizeof(type_name))fail("type-name-bound");for(unsigned i=0;i<s->Length/2;i++){if(s->Buffer[i]>127)fail("type-name-ascii");type_name[i]=char(s->Buffer[i]);}}
  LARGE_INTEGER zero{};wait=wait_nt(h,FALSE,&zero);dup_status=DuplicateHandle(GetCurrentProcess(),h,GetCurrentProcess(),&alias,0,FALSE,DUPLICATE_SAME_ACCESS)?0:LONG(GetLastError());close=close_nt(h);if(alias){alias_q=query(alias,0,basic,sizeof(basic),&br);alias_close=close_nt(alias);}if(qb<0||qt<0||close<0||dup_status!=0||alias_q<0||alias_close<0)fail("registration-resource-lifetime");
 }
 if(!GetProcessHandleCount(GetCurrentProcess(),&ha))fail("count-after");
 printf("case=%u width=%zu family=%u offset=%u value=%llX input-length=%lu output-length=%lu mode=%u status=%08lX returned=%08lX canary=%08lX handle=%llX query=%08lX basic-bytes=%lu grant=%08lX handles=%lu pointers=%lu type-query=%08lX type-bytes=%lu type=%s wait=%08lX duplicate=%08lX close=%08lX alias-query=%08lX alias-close=%08lX handles-before=%lu handles-after=%lu input-before=",ordinal++,sizeof(void*),family,offset,(unsigned long long)value,in_len,out_len,mode,ULONG(status),returned[0],returned[1],(unsigned long long)handle,ULONG(qb),br,grant,hc,pc,ULONG(qt),tr,type_name,ULONG(wait),ULONG(dup_status),ULONG(close),ULONG(alias_q),ULONG(alias_close),hb,ha);dump(before,192);printf(" input-after=");dump(input,192);printf(" output=");dump(output,192);puts("");if(hb!=ha||returned[1]!=0xB6B6B6B6)fail("resource-or-canary-mismatch");
}
int main(){setvbuf(stdout,nullptr,_IONBF,0);auto n=GetModuleHandleW(L"ntdll.dll");control=reinterpret_cast<Control>(GetProcAddress(n,"NtTraceControl"));query=reinterpret_cast<Query>(GetProcAddress(n,"NtQueryObject"));close_nt=reinterpret_cast<Close>(GetProcAddress(n,"NtClose"));wait_nt=reinterpret_cast<Wait>(GetProcAddress(n,"NtWaitForSingleObject"));if(!control||!query||!close_nt||!wait_nt)fail("exports");
 for(ULONG in_len:{0u,1u,4u,16u,24u,32u,48u,159u,160u,161u,168u,192u,320u})for(ULONG out_len:{0u,1u,4u,159u,160u,161u,192u,320u})one(0,0,0,in_len,out_len,0);
 for(ULONG v:{0u,1u,2u,3u,4u,5u,6u,7u,8u,16u,32u,65535u,0xFFFFFFFFu})one(1,16,v,160,160,0);
 for(ULONG v:{0u,1u,2u,3u,4u,16u,65536u,0xFFFFFFFFu})one(1,20,v,160,160,0);
 for(unsigned offset:{24u,32u})for(uint64_t v:{uint64_t(0),uint64_t(1),uint64_t(0x13572468),UINT64_MAX})one(2,offset,v,160,160,0);
 for(unsigned offset=40;offset<160;offset+=4)one(1,offset,0xFFFFFFFFu,160,160,0);
 for(unsigned offset=0;offset<16;offset+=4)one(3,offset,0xFF,160,160,0);
 one(4,0,0,160,160,0);
 for(unsigned mode:{1u,2u,3u,4u})for(ULONG out_len:{0u,159u,160u,192u})one(0,0,0,160,out_len,mode);
 printf("complete cases=%u\n",ordinal);return 0;
}
