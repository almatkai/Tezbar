// macOS-only differential probe for the exact pinned llama.cpp public C/C++ APIs.
// Compile against pinned source headers and a disposable runtime directory:
// clang++ -std=c++17 -O2 -I<SOURCE>/include -I<SOURCE>/src -I<SOURCE>/ggml/include \
//   tokenizer-parity.cpp -L<RUNTIME> -lllama -Wl,-rpath,<RUNTIME> -o <OUT>/parity
// Run: <OUT>/parity <READ_ONLY_MODEL.gguf> <OUT>/parity.sha256
// Compare the SHA-256 output across libraries. No token/model text is written.
// This is tokenizer/API parity, not inference, RAM or end-to-end app acceptance.
#include "llama.h"
#include "llama-vocab.h"
#include <fstream>
#include <CommonCrypto/CommonDigest.h>
#include <iomanip>
#include <iostream>
#include <thread>
#include <atomic>
#include <cstring>
#include <random>
#include <stdexcept>
#include <chrono>
struct hash_stream {
    CC_SHA256_CTX ctx; std::string path;
    void open(const char * p, std::ios::openmode) { path=p; CC_SHA256_Init(&ctx); }
    void write(const char * p, size_t n) { CC_SHA256_Update(&ctx,p,static_cast<CC_LONG>(n)); }
    void close() {
        unsigned char digest[CC_SHA256_DIGEST_LENGTH];CC_SHA256_Final(digest,&ctx);
        std::ofstream f(path);for(auto c:digest)f<<std::hex<<std::setw(2)<<std::setfill('0')<<static_cast<int>(c);f<<"\n";
    }
};
static hash_stream out;
template<class T> void put(T v) { out.write(reinterpret_cast<const char *>(&v),sizeof(v)); }
static void bytes(const std::string & v) { put<uint64_t>(v.size());out.write(v.data(),v.size()); }
static void check(bool b) { if(!b)throw std::runtime_error("parity harness invariant"); }
static std::vector<llama_token> tokenize(const llama_vocab * v,const std::string & s,bool add,bool parse) {
    int n=llama_tokenize(v,s.data(),s.size(),nullptr,0,add,parse);
    check(n<=0);
    std::vector<llama_token> result(-n);
    int actual=llama_tokenize(v,s.data(),s.size(),result.data(),result.size(),add,parse);
    check(actual==static_cast<int>(result.size()));return result;
}
int main(int argc,char ** argv) {
    check(argc==3);out.open(argv[2],std::ios::binary);
    llama_log_set([](ggml_log_level, const char *, void *){},nullptr);
    auto params=llama_model_default_params();params.vocab_only=true;
    llama_model * model=llama_model_load_from_file(argv[1],params);check(model);
    auto * v=llama_model_get_vocab(model);
    put<int>(llama_vocab_type(v));put<int>(llama_vocab_n_tokens(v));
    put<int>(llama_vocab_bos(v));put<int>(llama_vocab_eos(v));put<int>(llama_vocab_eot(v));
    put<int>(llama_vocab_sep(v));put<int>(llama_vocab_nl(v));put<int>(llama_vocab_pad(v));
    put<bool>(llama_vocab_get_add_bos(v));put<bool>(llama_vocab_get_add_eos(v));
    for (int id=0;id<llama_vocab_n_tokens(v);++id) {
        auto & t=v->get_token_data(id);
        bytes(t.text);bytes(llama_vocab_get_text(v,id));put<float>(llama_vocab_get_score(v,id));put<int>(llama_vocab_get_attr(v,id));
        put<int>(v->text_to_token(t.text));
        put<bool>(llama_vocab_is_eog(v,id));put<bool>(llama_vocab_is_control(v,id));
        auto & piece=v->token_to_piece(id);bytes(piece);
        for (bool special:{false,true}) for(int strip:{0,1,3}) {
            int n=llama_token_to_piece(v,id,nullptr,0,strip,special);put<int>(n);check(n<=0);
            std::string p(-n,'\0'); int len=llama_token_to_piece(v,id,p.data(),p.size(),strip,special);check(len==static_cast<int>(p.size())); bytes(p);
            if(!p.empty()) {
                std::string small(p.size()-1,'!');put<int>(llama_token_to_piece(v,id,small.data(),small.size(),strip,special));bytes(small);
            }
        }
    }
    for(int b=0;b<256;b++) { try {put<int>(v->byte_to_token(b));}catch(const std::exception & e) {put<int>(-999);bytes(e.what());} }
    auto merges=v->get_bpe_merges();put<uint64_t>(merges.size());
    for(auto & m:merges) {
        bytes(m);auto p=m.find(' ',1);
        if(p!=std::string::npos)put<int>(v->find_bpe_rank(m.substr(0,p),m.substr(p+1)));
        else put<int>(v->find_bpe_rank("",""));
    }
    std::mt19937 rng(1234567);
    for(int i=0;i<10000;i++) {
        auto & a=v->get_token_data(rng()%llama_vocab_n_tokens(v)).text;
        auto & b=v->get_token_data(rng()%llama_vocab_n_tokens(v)).text;
        if(a.find(' ')==std::string::npos && b.find(' ')==std::string::npos)put<int>(v->find_bpe_rank(a,b));
    }
    std::vector<std::string> texts={""," ","  \t\r\n", "\n\n a  b\t", "hello world", "қазақша Өрік алма іздеу", "Русский текст ёжик", "中文测试苹果 日本語検索 한국어", "العربية עברית हिन्दी ไทย", "é é ñ Straße 𐍈 😀👨‍👩‍👧‍👦", "<bos><eos><pad><unk><end_of_turn></s>","<|im_start|>user\nhi<|im_end|>", "a", "\xE2\x96\x81test", std::string("a\0b",3)};
    std::string all;for(int i=0;i<256;i++)all.push_back(static_cast<char>(i));texts.push_back(all);
    for(int i=0;i<256;i++){texts.push_back(std::string(1,static_cast<char>(i)));}
    for(int i=0;i<128;i++){std::string s;for(int j=0;j<1+(rng()%256);j++)s.push_back(rng()%256);texts.push_back(s);}
    std::string cjk;for(int i=0;i<1200;i++){unsigned c=0x4e00+i;cjk.push_back(0xe0|(c>>12));cjk.push_back(0x80|((c>>6)&63));cjk.push_back(0x80|(c&63));}texts.push_back(cjk);
    for(int n:{2048,4096,8192,8193}){std::string s;for(int i=0;i<n-3;i++)s+="a ";texts.push_back(s+"a");}
    // Model's own special/control tokens are exercised in context and alone.
    for(int id=0;id<llama_vocab_n_tokens(v);id++)if(llama_vocab_is_control(v,id)){texts.push_back(v->get_token_data(id).text);texts.push_back("before "+v->get_token_data(id).text+" after");}
    std::vector<std::vector<llama_token>> expected;
    std::vector<std::string> expected_text;
    for(auto & text:texts)for(bool add:{false,true})for(bool parse:{false,true}) {
        auto ts=tokenize(v,text,add,parse);put<uint64_t>(ts.size());for(auto t:ts)put<int>(t);expected.push_back(ts);
        int n=llama_detokenize(v,ts.data(),ts.size(),nullptr,0,add,parse);check(n<=0);put<int>(n);
        std::string dt(-n,'\0');int m=llama_detokenize(v,ts.data(),ts.size(),dt.data(),dt.size(),add,parse);check(m>=0 && m<=static_cast<int>(dt.size()));dt.resize(m);bytes(dt);expected_text.push_back(dt);
    }
    // Read-only reference/pointer stability under concurrent tokenize/detokenize.
    const auto * saved=&v->get_token_data(1000);const auto * piece=&v->token_to_piece(1000);const char * text=llama_vocab_get_text(v,1000);
    const std::string saved_text=saved->text,saved_piece=*piece;
    std::atomic<bool> ok(true);std::vector<std::thread> threads;
    for(int k=0;k<4;k++)threads.emplace_back([&,k] {
        size_t i=0;
        for(auto & s:texts)for(bool add:{false,true})for(bool parse:{false,true}) {
            if(i%4==static_cast<size_t>(k)) {
                auto ts=tokenize(v,s,add,parse);if(ts!=expected[i])ok=false;
                int n=llama_detokenize(v,ts.data(),ts.size(),nullptr,0,add,parse);
                if(n>0){ok=false;}else{
                    std::string dt(-n,'\0');int m=llama_detokenize(v,ts.data(),ts.size(),dt.data(),dt.size(),add,parse);
                    if(m<0 || m>static_cast<int>(dt.size()))ok=false;
                    else {dt.resize(m);if(dt!=expected_text[i])ok=false;}
                }
                if(v->get_token_data(1000).text!=saved_text || v->token_to_piece(1000)!=saved_piece)ok=false;
            }
            ++i;
        }
    });for(auto & t:threads)t.join();check(ok);
    check(saved==&v->get_token_data(1000)&&piece==&v->token_to_piece(1000)&&text==llama_vocab_get_text(v,1000));
    const int n_tokens=llama_vocab_n_tokens(v);
    out.close();llama_model_free(model);
    std::cout<<"tokens="<<n_tokens<<" merges="<<merges.size()<<" texts="<<texts.size()<<" concurrency=4 passed\n";
}
