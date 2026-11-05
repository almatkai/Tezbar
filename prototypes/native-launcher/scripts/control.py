#!/usr/bin/env python3
"""Allowlisted native controls; token is read from fixture, never CLI/logged."""
import argparse,json,pathlib,socket
class Control:
    def __init__(self,root): self.root=pathlib.Path(root)
    def command(self,op,**fields):
        request=json.dumps(dict(op=op,token=(self.root/'control-token').read_text().strip(),**fields)).encode()+b'\n'
        if (self.root/'control-pipe.txt').exists():
            with open((self.root/'control-pipe.txt').read_text(),'r+b',buffering=0) as s:
                s.write(request); data=b''
                while not data.endswith(b'\n'):
                    chunk=s.read(1)
                    if not chunk:raise RuntimeError('control pipe disconnected')
                    data+=chunk
        else:
            with socket.socket(socket.AF_UNIX) as s:
                s.settimeout(40);s.connect(str(self.root/'control.sock'));s.sendall(request);data=b''
                while not data.endswith(b'\n'):
                    chunk=s.recv(65536)
                    if not chunk:raise RuntimeError('control disconnected')
                    data+=chunk
        result=json.loads(data)
        if not result['ok']:raise RuntimeError(result['error'])
        return result['value']
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('root');p.add_argument('op',choices=['ready','status','show','hide','query','type','select','activation','quit']);p.add_argument('--text');p.add_argument('--index',type=int);a=p.parse_args()
    fields={k:v for k,v in dict(text=a.text,index=a.index).items() if v is not None}
    print(json.dumps(Control(a.root).command(a.op,**fields),indent=2))
