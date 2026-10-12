#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <cstdint>
#include <cstdio>
#pragma comment(lib,"advapi32.lib")
using Create=LONG(NTAPI *)(uint64_t *,ULONG,ULONG,BOOLEAN,void *,ULONG,void *);
using Delete=LONG(NTAPI *)(const uint64_t *);
using Subscribe=LONG(NTAPI *)(const uint64_t *,ULONG,ULONG,uint64_t *);
int main(){setvbuf(stdout,nullptr,_IONBF,0);auto m=GetModuleHandleW(L"ntdll.dll");auto create=reinterpret_cast<Create>(GetProcAddress(m,"NtCreateWnfStateName"));auto remove=reinterpret_cast<Delete>(GetProcAddress(m,"NtDeleteWnfStateName"));auto sub=reinterpret_cast<Subscribe>(GetProcAddress(m,"NtSubscribeWnfStateChange"));auto unsub=reinterpret_cast<Delete>(GetProcAddress(m,"NtUnsubscribeWnfStateChange"));if(!create||!remove||!sub||!unsub)return 2;SID_IDENTIFIER_AUTHORITY authority=SECURITY_NT_AUTHORITY;PSID system=nullptr;if(!AllocateAndInitializeSid(&authority,1,SECURITY_LOCAL_SYSTEM_RID,0,0,0,0,0,0,0,&system))return 3;unsigned count=0;
 for(DWORD grant=0;grant<4;grant++){unsigned char acl_bytes[128];auto acl=reinterpret_cast<PACL>(acl_bytes);SECURITY_DESCRIPTOR sd{};if(!InitializeAcl(acl,128,ACL_REVISION)||!AddAccessAllowedAce(acl,ACL_REVISION,grant,system)||!InitializeSecurityDescriptor(&sd,SECURITY_DESCRIPTOR_REVISION)||!SetSecurityDescriptorOwner(&sd,system,FALSE)||!SetSecurityDescriptorGroup(&sd,system,FALSE)||!SetSecurityDescriptorDacl(&sd,TRUE,acl,FALSE))return 4;uint64_t name=0;LONG status=create(&name,3,3,FALSE,nullptr,0,&sd);printf("created grant=%lu status=%08lX name=%016llX\n",grant,ULONG(status),(unsigned long long)name);if(status<0)return 5;
  for(ULONG mask=0;mask<32;mask++){unsub(&name);uint64_t id=0xA5A5A5A5A5A5A5A5;LONG a=sub(&name,0,mask,&id),end=unsub(&name);printf("case=%u width=%zu grant=%lu mask=%08lX status=%08lX id=%016llX end=%08lX\n",count++,sizeof(void *),grant,mask,ULONG(a),(unsigned long long)id,ULONG(end));}LONG deleted=remove(&name);printf("deleted grant=%lu status=%08lX\n",grant,ULONG(deleted));if(deleted<0)return 6;
 }FreeSid(system);printf("complete cases=%u created=4 deleted=4\n",count);return 0;}
