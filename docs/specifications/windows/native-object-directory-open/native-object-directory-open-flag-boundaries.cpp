#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <winternl.h>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <utility>
#include <initializer_list>
using Open=LONG(NTAPI*)(HANDLE*,ULONG,OBJECT_ATTRIBUTES*);using Close=LONG(NTAPI*)(HANDLE);
static Open open_nt;static Close close_nt;static unsigned count=0;
static void fail(const char* s){printf("failure=%s error=%lu\n",s,GetLastError());ExitProcess(21);}
static LONG call(HANDLE* out,OBJECT_ATTRIBUTES* attrs,ULONG* exception){__try{return open_nt(out,3,attrs);}__except(EXCEPTION_EXECUTE_HANDLER){*exception=GetExceptionCode();return LONG(0xDEADBEEF);}}
static DWORD protection(void* p){MEMORY_BASIC_INFORMATION m{};if(!VirtualQuery(p,&m,sizeof(m)))fail("query-protect");return m.Protect;}
static void protect(unsigned char* p,DWORD flags){DWORD old=0;if(!VirtualProtect(p,4096,flags,&old))fail("protect");}
static void one(unsigned ak,unsigned ok,unsigned nk,unsigned bk,ULONG length,USHORT name_length,ULONG flags){
 DWORD hb=0,ha=0;GetProcessHandleCount(GetCurrentProcess(),&hb);
 auto memory=static_cast<unsigned char*>(VirtualAlloc(nullptr,12*4096,MEM_RESERVE|MEM_COMMIT,PAGE_READWRITE));if(!memory)fail("allocate");memset(memory,0xA5,12*4096);
 auto attrs_page=memory;auto out_page=memory+2*4096;auto name_page=memory+4*4096;auto buffer_page=memory+6*4096;auto local=memory+10*4096;
 uintptr_t aa=uintptr_t(ak==0?local:ak==1?nullptr:ak==2?reinterpret_cast<void*>(1):ak==6?attrs_page+4096-sizeof(OBJECT_ATTRIBUTES)/2:ak>=7?local+(ak==7?1:ak==8?4:8):attrs_page);
 uintptr_t oa=uintptr_t(ok==0?local+256:ok==1?nullptr:ok==2?reinterpret_cast<void*>(1):ok==6?out_page+4096-sizeof(HANDLE)/2:ok>=7?local+256+(ok==7?1:ok==8?2:3):out_page);
 uintptr_t na=uintptr_t(nk==0?local+512:nk==1?nullptr:nk==2?reinterpret_cast<void*>(1):name_page);
 uintptr_t ba=uintptr_t(bk==0?local+1024:bk==1?nullptr:bk==2?reinterpret_cast<void*>(1):buffer_page);
 UNICODE_STRING name{};name.Length=name_length;name.MaximumLength=22;name.Buffer=reinterpret_cast<PWSTR>(ba);
 OBJECT_ATTRIBUTES attrs{};attrs.Length=length;attrs.ObjectName=reinterpret_cast<UNICODE_STRING*>(na);attrs.Attributes=flags;
 memcpy(local+(ak>=7?(ak==7?1:ak==8?4:8):0),&attrs,sizeof(attrs));memcpy(attrs_page,&attrs,sizeof(attrs));memcpy(attrs_page+4096-sizeof(attrs)/2,&attrs,sizeof(attrs));
 memcpy(local+512,&name,sizeof(name));memcpy(name_page,&name,sizeof(name));
 const wchar_t* path=L"\\KnownDlls";memcpy(local+1024,path,22);memcpy(buffer_page,path,22);
 std::pair<unsigned,unsigned char*> protected_pages[]={{ak,attrs_page},{ok,out_page},{nk,name_page},{bk,buffer_page}};
 for(auto pair:protected_pages){if(pair.first>=3&&pair.first<=5)protect(pair.second,pair.first==3?PAGE_READWRITE|PAGE_GUARD:pair.first==4?PAGE_NOACCESS:PAGE_READONLY);}
 if(ak==6)protect(attrs_page+4096,PAGE_NOACCESS);if(ok==6)protect(out_page+4096,PAGE_NOACCESS);
 ULONG exception=0;LONG status=call(reinterpret_cast<HANDLE*>(oa),reinterpret_cast<OBJECT_ATTRIBUTES*>(aa),&exception);
 DWORD ap=protection(attrs_page),aup=protection(attrs_page+4096),op=protection(out_page),oup=protection(out_page+4096),np=protection(name_page),bp=protection(buffer_page);
 DWORD ignored=0;if(!VirtualProtect(memory,12*4096,PAGE_READWRITE,&ignored))fail("unprotect-all");uintptr_t output=0;bool observed=ok!=1&&ok!=2;if(observed)memcpy(&output,reinterpret_cast<void*>(oa),sizeof(output));LONG closed=LONG(0xDEADBEEF);if(status>=0&&exception==0&&observed&&output){closed=close_nt(reinterpret_cast<HANDLE>(output));if(closed<0)fail("close");}
 VirtualFree(memory,0,MEM_RELEASE);GetProcessHandleCount(GetCurrentProcess(),&ha);
 printf("case=%u width=%zu attributes=%u output=%u unicode=%u buffer=%u attributes-length=%lu name-length=%u flags=%08lX status=%08lX exception=%08lX observed-output=%u output-after=%llX attributes-protect=%08lX attributes-upper-protect=%08lX output-protect=%08lX output-upper-protect=%08lX unicode-protect=%08lX buffer-protect=%08lX close=%08lX handles-before=%lu handles-after=%lu\n",count++,sizeof(void*),ak,ok,nk,bk,length,name_length,flags,ULONG(status),exception,observed,(unsigned long long)output,ap,aup,op,oup,np,bp,ULONG(closed),hb,ha);if(hb!=ha)fail("lifetime");
}
int main(){setvbuf(stdout,nullptr,_IONBF,0);auto n=GetModuleHandleW(L"ntdll.dll");open_nt=reinterpret_cast<Open>(GetProcAddress(n,"NtOpenDirectoryObject"));close_nt=reinterpret_cast<Close>(GetProcAddress(n,"NtClose"));if(!open_nt||!close_nt)fail("exports");
 for(ULONG flags:{0x40u,1u,0x20u,0x60u,0x100u,0xFFFFFFFFu})for(unsigned output:{0u,2u,3u})for(unsigned buffer=0;buffer<5;buffer++)for(USHORT length:{USHORT(0),USHORT(1),USHORT(20),USHORT(65534)})one(0,output,0,buffer,ULONG(sizeof(OBJECT_ATTRIBUTES)),length,flags);
 for(ULONG flags:{0x40u,1u,0x20u,0x60u,0x100u,0xFFFFFFFFu})for(unsigned output:{0u,3u})for(unsigned unicode=0;unicode<5;unicode++)one(0,output,unicode,0,ULONG(sizeof(OBJECT_ATTRIBUTES)),20,flags);
 printf("complete cases=%u\n",count);return 0;
}
