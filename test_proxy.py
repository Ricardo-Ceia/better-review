import pexpect
import time
import sys

print("Starting better-review opencode...")
start = time.time()
child = pexpect.spawn('./better-review opencode', encoding='utf-8')
child.logfile = sys.stdout

# Wait for opencode prompt or some output
child.expect(r'\[Better Review\] Proxy active', timeout=5)
print(f"Proxy active header seen at {time.time() - start:.2f}s")

time.sleep(1)
# Send Ctrl+O
print("Sending Ctrl+O")
child.send('\x0f')

time.sleep(1)
# Send q to quit review
print("Sending q")
child.send('q')

time.sleep(1)
child.send('\x03') # Ctrl+C
child.close()
print("Done")
