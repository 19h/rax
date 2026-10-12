#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <cstdio>
#include <vector>
#pragma comment(lib,"advapi32.lib")
static void sid(const char *kind,unsigned index,PSID value,DWORD attributes){auto bytes=static_cast<unsigned char *>(value);DWORD length=GetLengthSid(value);printf("sid kind=%s index=%u attributes=%08lX bytes=",kind,index,attributes);for(DWORD i=0;i<length;i++)printf("%02X",bytes[i]);puts("");}
int main(){setvbuf(stdout,nullptr,_IONBF,0);DWORD before=0,after=0;GetProcessHandleCount(GetCurrentProcess(),&before);HANDLE token=nullptr;if(!OpenProcessToken(GetCurrentProcess(),TOKEN_QUERY,&token))return 2;
 for(auto cls:{TokenUser,TokenGroups,TokenRestrictedSids,TokenIsAppContainer}){DWORD size=0;GetTokenInformation(token,cls,nullptr,0,&size);if(!size||size>65536)return 3;std::vector<unsigned char> b(size);if(!GetTokenInformation(token,cls,b.data(),size,&size))return 4;printf("token class=%u bytes=%lu\n",unsigned(cls),size);
  if(cls==TokenUser){auto u=reinterpret_cast<TOKEN_USER *>(b.data());sid("user",0,u->User.Sid,u->User.Attributes);}else if(cls==TokenIsAppContainer){printf("app-container=%lu\n",*reinterpret_cast<DWORD *>(b.data()));}else{auto g=reinterpret_cast<TOKEN_GROUPS *>(b.data());for(DWORD i=0;i<g->GroupCount;i++)sid(cls==TokenGroups?"group":"restricted",i,g->Groups[i].Sid,g->Groups[i].Attributes);}
 }CloseHandle(token);GetProcessHandleCount(GetCurrentProcess(),&after);printf("complete handles-before=%lu handles-after=%lu\n",before,after);return before==after?0:5;}
