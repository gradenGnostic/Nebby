"""Regression tests against the actual CTR donor service shared library."""
import ctypes as c
import hashlib
import sys

lib = c.CDLL(sys.argv[1])
lib.ctr_native_services_create.argtypes = [c.c_uint32] * 3 + [c.c_uint64]
lib.ctr_native_services_create.restype = c.c_void_p
lib.ctr_native_services_destroy.argtypes = [c.c_void_p]
Read = c.CFUNCTYPE(c.c_bool, c.c_void_p, c.c_uint32, c.c_void_p, c.c_size_t)
Write = Read
memory = bytearray(4096)

@Read
def read(_, address, output, size):
    if address < 0x1000 or address + size > 0x2000:
        return False
    if not output:
        return True
    c.memmove(output, bytes(memory[address-0x1000:address-0x1000+size]), size)
    return True

@Write
def write(_, address, source, size):
    if address < 0x1000 or address + size > 0x2000:
        return False
    if not source:
        return True
    memory[address-0x1000:address-0x1000+size] = c.string_at(source, size)
    return True

lib.ctr_native_services_dispatch.argtypes = [c.c_void_p, c.c_uint32,
    c.POINTER(c.c_uint32), c.c_size_t, c.c_void_p, Read, Write, c.c_uint32]
identity = 0x1234000123456789
runtime = lib.ctr_native_services_create(2, 1, 0, identity)
assert runtime

def dispatch(service, request):
    words = (c.c_uint32 * 64)(*request)
    result = lib.ctr_native_services_dispatch(runtime, service, words, 64, None, read, write, 0x1100)
    return result, list(words)

try:
    result, words = dispatch(0, [0x30040, 0xF1234567])
    assert result == 0 and words[:2] == [0x300C0, 0]
    expected = hashlib.sha256(identity.to_bytes(8, 'little') +
                             (0xF1234567 & 0xFFFFF).to_bytes(4, 'little')).digest()[24:]
    assert b''.join(w.to_bytes(4, 'little') for w in words[2:4]) == expected
    for command, output in [(0x20000, 2), (0x40000, 0), (0x50000, 0), (0x60000, 1)]:
        result, words = dispatch(0, [command])
        assert result == 0 and words[2] == output
    for block, size, expected in [(0xB0000, 4, bytes([0,0,0,110])),
                                  (0x30001, 8, bytes(8)),
                                  (0xC0001, 20, bytes(20)),
                                  (0xA0002, 1, bytes([1]))]:
        result, words = dispatch(0, [0x10082, size, block, (size<<4)|0xC, 0x1000])
        assert result == 0 and memory[:size] == expected
        assert words[:4] == [0x10042, 0, (size<<4)|0xC, 0x1000]
    assert dispatch(0, [0x10082, 4, 0xB0000, 0x48, 0x1000])[0] < 0
    assert dispatch(0, [0x10082, 4, 0xB0000, 0x4C, 0x3000])[0] < 0
    assert dispatch(0, [0x10082, 3, 0xB0000, 0x3C, 0x1000])[0] < 0
    assert dispatch(0, [0x10082, 4, 0xDEADBEEF, 0x4C, 0x1000])[0] < 0
    for request in [[0x140040, 15], [0x60040, 6], [0x60040, 2],
                    [0x70040, 2], [0x70040, 6]]:
        assert dispatch(1, request)[0] == 0
    assert dispatch(1, [0x70040, 2])[0] < 0
    assert dispatch(1, [0x10042,2,0x20,7])[0] == 0
    assert dispatch(1, [0x30000])[1][2] == 2
    assert dispatch(1, [0x10042,1,0x20,8])[0] < 0
    assert dispatch(1, [0x20002,0x20,8])[0] < 0
    assert dispatch(1, [0x20002,0x20,7])[0] == 0
    assert dispatch(1, [0x30000])[1][2] == 0
    # Real native APT lifecycle; no window/mock service is involved.
    result, words = dispatch(2, [0xE0080, 0x300, 16])
    assert result == 0 and words[:2] == [0xE0040, 0xC8A0CFFC]
    result, words = dispatch(2, [0x10040, 0])
    assert result == 0 and words[0] == 0x100C2 and words[5]
    result, words = dispatch(2, [0x20080, 0x300, 0])
    assert result == 0 and words[0] == 0x20043 and words[3] != words[4]
    memory[0x100:0x108] = (16<<14|2).to_bytes(4,'little') + (0x1200).to_bytes(4,'little')
    memory[0x200:0x210] = b'\xA5' * 16
    for request in [[0xE0080,0x300,16], [0xE0080,0x300,16], [0xD0080,0x300,16]]:
        result, words = dispatch(2, request)
        assert result == 0 and words[1:5] == [0,0,1,0] and words[7:9] == [2,0x1200]
        assert memory[0x200:0x210] == b'\xA5' * 16
    assert dispatch(2, [0xD0080,0x300,16])[1][1] == 0xC8A0CFFC
    assert dispatch(2, [0x550040,2])[0] == 0
    assert dispatch(2, [0x560000])[1][2] == 2
    assert dispatch(2, [0x550040,4])[0] < 0
    assert dispatch(2, [0x4B00C2,7,4,1,0x10402,0x1300])[0] == 0
    assert memory[0x200] == 0 and memory[0x201] == 0xA5
    # Real portable YUV420 -> tiled RGBA conversion with guest memory callbacks.
    assert dispatch(3,[0x2B0000])[0] == 0
    result, words = dispatch(3,[0xF0000])
    assert result == 0 and words[0] == 0xF0042 and words[3]
    completion_handle = words[3]
    for command, value in [(1,1),(3,0),(5,0),(7,1),(0x1A,8),(0x1C,8),(0x20,2),(0x22,255)]:
        result, words = dispatch(3,[(command<<16)|0x40,value])
        assert result == 0 and words[1] == 0
    memory[0x800:0x840] = bytes([128])*64
    memory[0x900:0x910] = bytes([128])*16
    memory[0xA00:0xA10] = bytes([128])*16
    for command,address,size,unit in [(0x10,0x1800,64,8),(0x11,0x1900,16,4),
                                      (0x12,0x1A00,16,4),(0x18,0x1C00,256,256)]:
        assert dispatch(3,[(command<<16)|0x102,address,size,unit,0,0,0xFFFF8001])[1][1] == 0
    assert dispatch(3,[0x260000])[1][1] == 0
    assert memory[0xC00:0xD00] == bytes([255,130,131,130])*64, memory[0xC00:0xC10].hex()
    lib.ctr_native_services_object.argtypes = [c.c_void_p,c.c_uint32,c.POINTER(c.c_uint32)]
    metadata = (c.c_uint32*3)()
    assert lib.ctr_native_services_object(runtime,completion_handle,metadata) == 0
    assert list(metadata) == [0,1,1]
    assert dispatch(3,[0x180102,0x3000,256,256,0,0,0xFFFF8001])[1][1] == 0
    assert dispatch(3,[0x260000])[1][1] != 0
    assert memory[0xC00:0xD00] == bytes([255,130,131,130])*64
    assert dispatch(3,[0x100102,0x1800,64,8,0,0x20,0])[0] < 0
    assert dispatch(4,[0x320042,0xB0401C8,0x20,10])[1][1] == 0
    assert dispatch(4,[0x320042,0xB0401C8,0,10])[0] < 0
    assert dispatch(4,[0x50000])[1][:6] == [0x50140,0,0,0,0,0]
    memory[0x100:0x108] = (0x12C<<14|2).to_bytes(4,'little') + (0x1200).to_bytes(4,'little')
    memory[0x200:0x32C] = b'\xA5'*0x12C
    result,words = dispatch(4,[0x80000])
    assert result == 0 and words[:4] == [0x80042,0,0x12C<<14|2,0x1200]
    assert memory[0x200:0x32C] == bytes(0x12C)  # Offline, no advertised game/join.
    memory[0x100:0x108] = (1600<<14|2).to_bytes(4,'little') + (0x1200).to_bytes(4,'little')
    result,words = dispatch(4,[0x110080,0,100])
    assert result == 0 and words[:5] == [0x110082,0,0,1600<<14|2,0x1200]
    assert memory[0x200:0x840] == bytes(1600)
    assert dispatch(4,[0x110080,0,101])[0] < 0
    assert dispatch(5,[0x10002,0x20,10])[1][:2] == [0x10040,0]
    memory[0xD00:0xD40] = b'\xA5'*64
    assert dispatch(5,[0x110042,32,(64<<4)|12,0x1D00])[1][:4] == [0x110042,0,(64<<4)|12,0x1D00]
    random = bytes(memory[0xD00:0xD20])
    assert random != bytes(32) and memory[0xD20:0xD40] == b'\xA5'*32
    assert dispatch(5,[0x110042,32,(64<<4)|12,0x1D00])[0] == 0
    assert bytes(memory[0xD00:0xD20]) != random
    assert dispatch(5,[0x110042,32,(64<<4)|10,0x1D00])[0] < 0
    assert dispatch(5,[0x110042,65,(64<<4)|12,0x1D00])[0] < 0
    assert dispatch(5,[0x110042,32,(64<<4)|12,0x3000])[0] < 0
    assert dispatch(6,[0x1B0302]+[0]*14)[1][:2] == [0x1B0040,0xC9411002]
    assert dispatch(6,[0x1B0302]+[0]*12+[0x20,0])[0] < 0
    print('native CFG/NDM/APT/Y2R/Friends/SSL/UDS contract tests passed')
finally:
    lib.ctr_native_services_destroy(runtime)
