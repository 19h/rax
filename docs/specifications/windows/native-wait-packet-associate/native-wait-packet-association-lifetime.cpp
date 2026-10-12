#include <windows.h>
#include <winternl.h>
#include <cstdio>
#include <cstring>
#include <initializer_list>
using Create=LONG(NTAPI*)(PHANDLE,ACCESS_MASK,POBJECT_ATTRIBUTES);
using Port=LONG(NTAPI*)(PHANDLE,ACCESS_MASK,POBJECT_ATTRIBUTES,ULONG);
using Event=LONG(NTAPI*)(PHANDLE,ACCESS_MASK,POBJECT_ATTRIBUTES,ULONG,BOOLEAN);
using Timer=LONG(NTAPI*)(PHANDLE,PULONG,POBJECT_ATTRIBUTES,ULONG,ACCESS_MASK);
using Associate=LONG(NTAPI*)(HANDLE,HANDLE,HANDLE,PVOID,PVOID,LONG,ULONG_PTR,PBOOLEAN);
using Cancel=LONG(NTAPI*)(HANDLE,BOOLEAN);
using Remove=LONG(NTAPI*)(HANDLE,PVOID*,PVOID*,PIO_STATUS_BLOCK,PLARGE_INTEGER);
using Set=LONG(NTAPI*)(HANDLE,PLONG);
using Close=LONG(NTAPI*)(HANDLE);
static Create packet;static Port port;static Event event;static Timer timer;
static Associate associate;static Cancel cancel;static Remove remove_packet;static Set set_event;static Close close_object;
static unsigned ordinal;
static DWORD wait(HANDLE h){return WaitForSingleObject(h,0);}
static void run(unsigned kind,BOOLEAN signaled,int cancel_mode){
 DWORD before=0,after=0;GetProcessHandleCount(GetCurrentProcess(),&before);
 HANDLE w=nullptr,p=nullptr,t=nullptr;
 LONG a=packet(&w,0x1F0001,nullptr),b=port(&p,0x1F0003,nullptr,1);
 LONG c=kind==2?timer(&t,nullptr,nullptr,8,0x1F0003):event(&t,0x1F0003,nullptr,kind,signaled);
 if(a<0||b<0||c<0)ExitProcess(41);
 unsigned char already[16];std::memset(already,0xA5,sizeof already);
 DWORD initial=wait(w);
 LONG status=associate(w,p,t,reinterpret_cast<void*>(ULONG_PTR(0x1234567887654321ull)),reinterpret_cast<void*>(ULONG_PTR(0xA1B2C3D4CCCCDDDDull)),LONG(0xC1234567),ULONG_PTR(0xFEDCBA9876543210ull),already);
 DWORD associated=wait(w);LONG cancel_status=LONG(0xDEADBEEF);
 if(cancel_mode>=0)cancel_status=cancel(w,BOOLEAN(cancel_mode));
 DWORD after_cancel=wait(w);LONG set_status=LONG(0xDEADBEEF);
 if(kind!=2)set_status=set_event(t,nullptr);
 DWORD after_signal=wait(w);
 PVOID key=reinterpret_cast<void*>(ULONG_PTR(0xA5A5A5A5A5A5A5A5ull)),apc=key;
 IO_STATUS_BLOCK io;std::memset(&io,0xA5,sizeof io);LARGE_INTEGER zero{};
 LONG removed=remove_packet(p,&key,&apc,&io,&zero);DWORD after_remove=wait(w);
 LONG canceled_again=cancel(w,TRUE);DWORD final_wait=wait(w);
 LONG close_t=close_object(t),close_p=close_object(p),close_w=close_object(w);
 GetProcessHandleCount(GetCurrentProcess(),&after);
 std::printf("case=%u width=%zu kind=%u signaled=%u cancel-mode=%d status=%08lX initial=%08lX associated=%08lX cancel=%08lX after-cancel=%08lX set=%08lX after-signal=%08lX removed=%08lX key=%llX apc=%llX io-status=%08lX information=%llX after-remove=%08lX cancel-again=%08lX final-wait=%08lX close-target=%08lX close-port=%08lX close-packet=%08lX handles-before=%lu handles-after=%lu already=",ordinal++,sizeof(void*),kind,signaled,cancel_mode,ULONG(status),initial,associated,ULONG(cancel_status),after_cancel,ULONG(set_status),after_signal,ULONG(removed),(unsigned long long)ULONG_PTR(key),(unsigned long long)ULONG_PTR(apc),ULONG(io.Status),(unsigned long long)io.Information,after_remove,ULONG(canceled_again),final_wait,ULONG(close_t),ULONG(close_p),ULONG(close_w),before,after);
 for(unsigned char byte:already)std::printf("%02X",unsigned(byte));std::printf("\n");
 if(close_t<0||close_p<0||close_w<0||before!=after)ExitProcess(42);
}

static LONG safe_associate(HANDLE w,HANDLE p,HANDLE t,PBOOLEAN out,LONG* exception){
 __try{return associate(w,p,t,reinterpret_cast<void*>(ULONG_PTR(0x1234567887654321ull)),reinterpret_cast<void*>(ULONG_PTR(0xA1B2C3D4CCCCDDDDull)),LONG(0xC1234567),ULONG_PTR(0xFEDCBA9876543210ull),out);}
 __except(EXCEPTION_EXECUTE_HANDLER){*exception=GetExceptionCode();return LONG(0xDEADBEEF);}
}
static HANDLE make(unsigned role,ACCESS_MASK rights){HANDLE h=nullptr;LONG st=role==0?packet(&h,rights,nullptr):role==1?port(&h,rights,nullptr,1):event(&h,rights,nullptr,0,FALSE);if(st<0)ExitProcess(44);return h;}
static void validation(unsigned role,unsigned mode,unsigned pointer){
 DWORD before=0,after=0;GetProcessHandleCount(GetCurrentProcess(),&before);
 HANDLE h[3]={make(0,0x1F0001),make(1,0x1F0003),make(2,0x1F0003)},extra=nullptr;
 HANDLE chosen=h[role];
 if(mode==1)chosen=nullptr;if(mode==2)chosen=reinterpret_cast<HANDLE>(ULONG_PTR(0x3330));
 if(mode==3||mode==4){extra=make(2,mode==3?0x1F0003:0);chosen=extra;}
 if(mode>=5&&mode<=8){ACCESS_MASK grants[]={0,1,2,0x100000};extra=make(role,grants[mode-5]);chosen=extra;}
 if(mode==9)chosen=GetCurrentProcess();if(mode==10)chosen=GetCurrentThread();
 unsigned char* region=(unsigned char*)VirtualAlloc(nullptr,8192,MEM_RESERVE|MEM_COMMIT,PAGE_READWRITE);
 if(!region)ExitProcess(45);std::memset(region,0xA5,8192);DWORD old;
 PBOOLEAN output=region;
 if(pointer==1)output=nullptr;
 if(pointer==2)output=reinterpret_cast<PBOOLEAN>(ULONG_PTR(1));
 if(pointer==3){VirtualProtect(region,4096,PAGE_READONLY,&old);}
 if(pointer==4){VirtualProtect(region,4096,PAGE_READWRITE|PAGE_GUARD,&old);}
 if(pointer==5)output=region+1;
 if(pointer==6){output=region+4095;VirtualProtect(region+4096,4096,PAGE_NOACCESS,&old);}
 HANDLE args[3]={h[0],h[1],h[2]};args[role]=chosen;LONG exception=0;
 LONG st=safe_associate(args[0],args[1],args[2],output,&exception);
 MEMORY_BASIC_INFORMATION info{};VirtualQuery(region,&info,sizeof(info));DWORD protection=info.Protect;
 VirtualProtect(region,8192,PAGE_READWRITE,&old);
 unsigned first=region[0],last=region[4095],unaligned=region[1];
 LONG canceled=cancel(h[0],TRUE);
 LONG cancel_extra=extra&&role==0?cancel(extra,TRUE):LONG(0xDEADBEEF);
 if(extra)close_object(extra);for(auto v:h)close_object(v);VirtualFree(region,0,MEM_RELEASE);
 GetProcessHandleCount(GetCurrentProcess(),&after);
 std::printf("validation=%u width=%zu role=%u mode=%u pointer=%u status=%08lX exception=%08lX first=%02X unaligned=%02X last=%02X protection=%08lX cancel=%08lX cancel-extra=%08lX handles-before=%lu handles-after=%lu\n",ordinal++,sizeof(void*),role,mode,pointer,ULONG(st),ULONG(exception),first,unaligned,last,protection,ULONG(canceled),ULONG(cancel_extra),before,after);
 if(before!=after)ExitProcess(46);
}
static void op(const char* name,HANDLE w,HANDLE p,HANDLE t,int action){
 unsigned char already[16];std::memset(already,0xA5,sizeof(already));LONG exception=0,st;
 if(action<0)st=safe_associate(w,p,t,already,&exception);else st=cancel(w,BOOLEAN(action));
 std::printf("state=%s width=%zu status=%08lX exception=%08lX already=%02X packet-wait=%08lX\n",name,sizeof(void*),ULONG(st),ULONG(exception),unsigned(already[0]),wait(w));
}
static void states(BOOLEAN initial){
 DWORD before=0,after=0;GetProcessHandleCount(GetCurrentProcess(),&before);
 HANDLE w=make(0,0x1F0001),p=make(1,0x1F0003),t=nullptr;event(&t,0x1F0003,nullptr,1,initial);
 std::printf("scenario=%u\n",unsigned(initial));
 op("fresh-cancel",w,p,t,0);op("first",w,p,t,-1);op("second",w,p,t,-1);
 op("cancel-false",w,p,t,0);op("after-false",w,p,t,-1);op("cancel-true",w,p,t,1);
 op("after-true",w,p,t,-1);set_event(t,nullptr);op("after-set",w,p,t,-1);
 PVOID key=nullptr,apc=nullptr;IO_STATUS_BLOCK io{};LARGE_INTEGER zero{};
 LONG st=remove_packet(p,&key,&apc,&io,&zero);
 std::printf("state=remove width=%zu status=%08lX key=%llX apc=%llX io-status=%08lX information=%llX target-wait=%08lX\n",sizeof(void*),ULONG(st),(unsigned long long)ULONG_PTR(key),(unsigned long long)ULONG_PTR(apc),ULONG(io.Status),(unsigned long long)io.Information,wait(t));
 op("after-remove",w,p,t,-1);op("cancel-two",w,p,t,2);op("cancel-again",w,p,t,255);
 close_object(t);close_object(p);close_object(w);GetProcessHandleCount(GetCurrentProcess(),&after);
 std::printf("scenario-complete=%u handles-before=%lu handles-after=%lu\n",unsigned(initial),before,after);if(before!=after)ExitProcess(47);
}

using Query=LONG(NTAPI*)(HANDLE,ULONG,PVOID,ULONG,PULONG);static Query query;
static void counts(const char* label,HANDLE p,HANDLE t){ULONG pb[32]{},tb[32]{},len=0;LONG a=query(p,0,pb,sizeof(pb),&len),c=query(t,0,tb,sizeof(tb),&len);std::printf("counts=%s port-status=%08lX port-handles=%lu port-pointers=%lu target-status=%08lX target-handles=%lu target-pointers=%lu length=%lu\n",label,ULONG(a),pb[2],pb[3],ULONG(c),tb[2],tb[3],len);}
static void lifetime(BOOLEAN initial,unsigned closemode){
 DWORD before=0,after=0;GetProcessHandleCount(GetCurrentProcess(),&before);HANDLE w=make(0,0x1F0001),p=make(1,0x1F0003),t=nullptr,alias=nullptr;event(&t,0x1F0003,nullptr,0,initial);
 if(closemode==1&&!DuplicateHandle(GetCurrentProcess(),w,GetCurrentProcess(),&alias,0,FALSE,DUPLICATE_SAME_ACCESS))ExitProcess(48);
 std::printf("lifetime=%u closemode=%u width=%zu\n",unsigned(initial),closemode,sizeof(void*));counts("before",p,t);
 op("associate",w,p,t,-1);counts("associated",p,t);
 if(closemode<2)close_object(w);else cancel(w,TRUE);counts("after-close-or-cancel",p,t);
 if(alias){close_object(alias);counts("after-alias",p,t);}
 set_event(t,nullptr);counts("after-signal",p,t);
 PVOID key=nullptr,apc=nullptr;IO_STATUS_BLOCK io{};LARGE_INTEGER zero{};LONG st=remove_packet(p,&key,&apc,&io,&zero);
 std::printf("lifetime-remove=%08lX io-status=%08lX information=%llX\n",ULONG(st),ULONG(io.Status),(unsigned long long)io.Information);counts("after-remove",p,t);
 if(closemode==2)close_object(w);close_object(p);close_object(t);GetProcessHandleCount(GetCurrentProcess(),&after);std::printf("lifetime-complete handles-before=%lu handles-after=%lu\n",before,after);if(before!=after)ExitProcess(49);
}
static void target_kind(unsigned kind){
 DWORD before=0,after=0;GetProcessHandleCount(GetCurrentProcess(),&before);HANDLE w=make(0,0x1F0001),p=make(1,0x1F0003),t=nullptr;bool real=true;LONG created=0;
 switch(kind){case 0:case 1:created=event(&t,0x1F0003,nullptr,kind,TRUE);break;
 case 2:t=CreateWaitableTimerW(nullptr,FALSE,nullptr);break;case 3:created=timer(&t,nullptr,nullptr,8,0x1F0003);break;
 case 4:t=CreateMutexW(nullptr,FALSE,nullptr);break;case 5:t=CreateSemaphoreW(nullptr,1,2,nullptr);break;case 6:t=CreateSemaphoreW(nullptr,0,2,nullptr);break;
 case 7:t=GetCurrentThread();real=false;break;case 8:t=GetCurrentProcess();real=false;break;case 9:t=make(0,0x1F0001);break;case 10:t=make(1,0x1F0003);break;
 case 11:t=CreateFileW(L"NUL",GENERIC_READ,FILE_SHARE_READ|FILE_SHARE_WRITE,nullptr,OPEN_EXISTING,0,nullptr);break;
 case 12:t=CreateFileMappingW(INVALID_HANDLE_VALUE,nullptr,PAGE_READWRITE,0,4096,nullptr);break;
 }
 if(created<0||!t||t==INVALID_HANDLE_VALUE&&real)ExitProcess(50);
 unsigned char already[16];std::memset(already,0xA5,sizeof(already));LONG exception=0,st=safe_associate(w,p,t,already,&exception);LONG canceled=cancel(w,TRUE);DWORD tw=wait(t);
 if(real)close_object(t);close_object(p);close_object(w);GetProcessHandleCount(GetCurrentProcess(),&after);
 std::printf("target-kind=%u width=%zu status=%08lX exception=%08lX already=%02X cancel=%08lX target-wait=%08lX handles-before=%lu handles-after=%lu\n",kind,sizeof(void*),ULONG(st),ULONG(exception),unsigned(already[0]),ULONG(canceled),tw,before,after);if(before!=after)ExitProcess(51);
}
static void cancel_validation(unsigned mode,unsigned flag){
 DWORD before=0,after=0;GetProcessHandleCount(GetCurrentProcess(),&before);HANDLE real=nullptr,h=nullptr;
 if(mode==0){h=real=make(0,0x1F0001);}if(mode==1)h=nullptr;if(mode==2)h=reinterpret_cast<HANDLE>(ULONG_PTR(0x3330));
 if(mode==3||mode==4)h=real=make(2,mode==3?0x1F0003:0);
 if(mode>=5&&mode<=8){ACCESS_MASK grants[]={0,1,2,0x100000};h=real=make(0,grants[mode-5]);}
 if(mode==9)h=GetCurrentProcess();if(mode==10)h=GetCurrentThread();LONG st=cancel(h,BOOLEAN(flag));if(real)close_object(real);
 GetProcessHandleCount(GetCurrentProcess(),&after);std::printf("cancel-mode=%u flag=%u width=%zu status=%08lX handles-before=%lu handles-after=%lu\n",mode,flag,sizeof(void*),ULONG(st),before,after);if(before!=after)ExitProcess(52);
}
int main(){auto n=GetModuleHandleW(L"ntdll.dll");
#define LOAD(variable,type,name) variable=reinterpret_cast<type>(GetProcAddress(n,name));if(!variable)return 43
 LOAD(packet,Create,"NtCreateWaitCompletionPacket");LOAD(port,Port,"NtCreateIoCompletion");LOAD(event,Event,"NtCreateEvent");LOAD(timer,Timer,"NtCreateTimer2");LOAD(associate,Associate,"NtAssociateWaitCompletionPacket");LOAD(cancel,Cancel,"NtCancelWaitCompletionPacket");LOAD(remove_packet,Remove,"NtRemoveIoCompletion");LOAD(set_event,Set,"NtSetEvent");LOAD(close_object,Close,"NtClose");LOAD(query,Query,"NtQueryObject");
 for(BOOLEAN initial:{BOOLEAN(0),BOOLEAN(1)})for(unsigned mode=0;mode<3;++mode)lifetime(initial,mode);
 for(unsigned kind=0;kind<=12;++kind)target_kind(kind);
 for(unsigned mode=0;mode<=10;++mode)for(unsigned flag:{0u,1u,2u,128u,255u})cancel_validation(mode,flag);
 std::printf("complete lifetime=6 targets=13 cancel=55\n");return 0;
}
