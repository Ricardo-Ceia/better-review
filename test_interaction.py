import pty
import os
import sys
import time

pid, fd = pty.fork()

if pid == 0:
    # Child process
    os.execvp("./better-review", ["better-review", "./mock_opencode"])
else:
    # Parent process
    out = os.read(fd, 1024)
    print("GOT:", out.decode('utf-8', errors='replace'))
    time.sleep(1)
    
    print("SENDING CTRL+O")
    os.write(fd, b'\x0f')
    
    time.sleep(1)
    out2 = os.read(fd, 1024)
    print("GOT AFTER CTRL+O:", out2.decode('utf-8', errors='replace'))
    
    print("SENDING Q")
    os.write(fd, b'q')
    
    time.sleep(1)
    out3 = os.read(fd, 1024)
    print("GOT AFTER Q:", out3.decode('utf-8', errors='replace'))
    
    os.kill(pid, 9)
