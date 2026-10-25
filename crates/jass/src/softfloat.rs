//! The original's software floats.  JASS reals are added, multiplied, divided, parsed, and
//! printed by game.dll's own float routines, which truncate where IEEE arithmetic rounds
//! and divide by a tabled reciprocal (measured: 1.1 * 1.1 prints 1.209999680 there, and
//! R2S(0.9995) prints 1.000).  These work on the bit patterns as those routines do.

use std::sync::OnceLock;

/// The reciprocal's table, as game.dll has it (at 0x6fa810c0): 2^23 (1024 - i) / (1024 + i)
/// near enough, but tuned in its last bit, so given here as it is.
#[rustfmt::skip]
static RECIPROCAL: [u32; 1025] = [
    0x800000, 0x7fc011, 0x7f8041, 0x7f4091, 0x7f0100, 0x7ec190, 0x7e823e, 0x7e430c,
    0x7e03fa, 0x7dc506, 0x7d8632, 0x7d477d, 0x7d08e7, 0x7cca70, 0x7c8c17, 0x7c4ddd,
    0x7c0fc2, 0x7bd1c6, 0x7b93e8, 0x7b5628, 0x7b1887, 0x7adb04, 0x7a9d9f, 0x7a6057,
    0x7a232e, 0x79e623, 0x79a936, 0x796c66, 0x792fb4, 0x78f31f, 0x78b6a8, 0x787a4e,
    0x783e11, 0x7801f1, 0x77c5ef, 0x778a0a, 0x774e41, 0x771296, 0x76d707, 0x769b95,
    0x76603f, 0x762506, 0x75e9e9, 0x75aee9, 0x757405, 0x75393d, 0x74fe92, 0x74c402,
    0x74898f, 0x744f37, 0x7414fb, 0x73dadb, 0x73a0d6, 0x7366ee, 0x732d20, 0x72f36e,
    0x72b9d8, 0x72805c, 0x7246fc, 0x720db7, 0x71d48d, 0x719b7e, 0x71628a, 0x7129b1,
    0x70f0f2, 0x70b84e, 0x707fc5, 0x704756, 0x700f02, 0x6fd6c8, 0x6f9ea9, 0x6f66a3,
    0x6f2eb8, 0x6ef6e7, 0x6ebf30, 0x6e8793, 0x6e5010, 0x6e18a7, 0x6de157, 0x6daa21,
    0x6d7305, 0x6d3c02, 0x6d0519, 0x6cce49, 0x6c9792, 0x6c60f5, 0x6c2a71, 0x6bf406,
    0x6bbdb4, 0x6b877b, 0x6b515b, 0x6b1b54, 0x6ae565, 0x6aaf90, 0x6a79d2, 0x6a442e,
    0x6a0ea2, 0x69d92f, 0x69a3d4, 0x696e91, 0x693966, 0x690454, 0x68cf5a, 0x689a78,
    0x6865ae, 0x6830fb, 0x67fc61, 0x67c7df, 0x679374, 0x675f21, 0x672ae6, 0x66f6c2,
    0x66c2b5, 0x668ec1, 0x665ae3, 0x66271d, 0x65f36e, 0x65bfd6, 0x658c55, 0x6558ec,
    0x652599, 0x64f25e, 0x64bf39, 0x648c2b, 0x645934, 0x642654, 0x63f38a, 0x63c0d7,
    0x638e3a, 0x635bb4, 0x632944, 0x62f6eb, 0x62c4a8, 0x62927b, 0x626064, 0x622e64,
    0x61fc79, 0x61caa5, 0x6198e6, 0x61673e, 0x6135ab, 0x61042e, 0x60d2c7, 0x60a175,
    0x607039, 0x603f13, 0x600e02, 0x5fdd06, 0x5fac20, 0x5f7b50, 0x5f4a94, 0x5f19ee,
    0x5ee95d, 0x5eb8e1, 0x5e887b, 0x5e5829, 0x5e27ec, 0x5df7c4, 0x5dc7b1, 0x5d97b3,
    0x5d67ca, 0x5d37f5, 0x5d0835, 0x5cd889, 0x5ca8f2, 0x5c7970, 0x5c4a02, 0x5c1aa8,
    0x5beb63, 0x5bbc32, 0x5b8d15, 0x5b5e0d, 0x5b2f18, 0x5b0038, 0x5ad16b, 0x5aa2b3,
    0x5a740f, 0x5a457e, 0x5a1701, 0x59e898, 0x59ba43, 0x598c02, 0x595dd4, 0x592fba,
    0x5901b3, 0x58d3c0, 0x58a5e0, 0x587814, 0x584a5a, 0x581cb5, 0x57ef22, 0x57c1a3,
    0x579437, 0x5766de, 0x573998, 0x570c65, 0x56df45, 0x56b238, 0x56853e, 0x565856,
    0x562b82, 0x55fec0, 0x55d211, 0x55a574, 0x5578ea, 0x554c73, 0x55200e, 0x54f3bc,
    0x54c77c, 0x549b4e, 0x546f33, 0x54432a, 0x541733, 0x53eb4f, 0x53bf7d, 0x5393bc,
    0x53680e, 0x533c72, 0x5310e8, 0x52e56f, 0x52ba09, 0x528eb5, 0x526372, 0x523841,
    0x520d22, 0x51e214, 0x51b718, 0x518c2e, 0x516155, 0x51368e, 0x510bd8, 0x50e134,
    0x50b6a1, 0x508c1f, 0x5061af, 0x503750, 0x500d02, 0x4fe2c5, 0x4fb899, 0x4f8e7f,
    0x4f6475, 0x4f3a7d, 0x4f1096, 0x4ee6bf, 0x4ebcfa, 0x4e9345, 0x4e69a1, 0x4e400e,
    0x4e168b, 0x4ded1a, 0x4dc3b8, 0x4d9a68, 0x4d7128, 0x4d47f9, 0x4d1eda, 0x4cf5cc,
    0x4cccce, 0x4ca3e0, 0x4c7b03, 0x4c5236, 0x4c2979, 0x4c00cd, 0x4bd831, 0x4bafa4,
    0x4b8729, 0x4b5ebd, 0x4b3661, 0x4b0e15, 0x4ae5d9, 0x4abdad, 0x4a9591, 0x4a6d85,
    0x4a4589, 0x4a1d9c, 0x49f5bf, 0x49cdf2, 0x49a635, 0x497e87, 0x4956e9, 0x492f5a,
    0x4907db, 0x48e06b, 0x48b90b, 0x4891bb, 0x486a79, 0x484347, 0x481c25, 0x47f511,
    0x47ce0d, 0x47a718, 0x478033, 0x47595c, 0x473295, 0x470bdc, 0x46e533, 0x46be98,
    0x46980d, 0x467191, 0x464b23, 0x4624c5, 0x45fe75, 0x45d834, 0x45b201, 0x458bde,
    0x4565c9, 0x453fc3, 0x4519cc, 0x44f3e3, 0x44ce08, 0x44a83d, 0x44827f, 0x445cd1,
    0x443730, 0x44119e, 0x43ec1b, 0x43c6a5, 0x43a13f, 0x437be6, 0x43569c, 0x43315f,
    0x430c31, 0x42e712, 0x42c200, 0x429cfc, 0x427807, 0x42531f, 0x422e46, 0x42097a,
    0x41e4bc, 0x41c00d, 0x419b6b, 0x4176d7, 0x415251, 0x412dd8, 0x41096e, 0x40e511,
    0x40c0c1, 0x409c80, 0x40784c, 0x405425, 0x40300d, 0x400c01, 0x3fe804, 0x3fc413,
    0x3fa031, 0x3f7c5b, 0x3f5893, 0x3f34d8, 0x3f112b, 0x3eed8b, 0x3ec9f8, 0x3ea673,
    0x3e82fb, 0x3e5f90, 0x3e3c32, 0x3e18e1, 0x3df59d, 0x3dd267, 0x3daf3d, 0x3d8c20,
    0x3d6911, 0x3d460e, 0x3d2319, 0x3d0030, 0x3cdd54, 0x3cba85, 0x3c97c3, 0x3c750d,
    0x3c5265, 0x3c2fc9, 0x3c0d3a, 0x3beab7, 0x3bc841, 0x3ba5d8, 0x3b837b, 0x3b612b,
    0x3b3ee8, 0x3b1cb1, 0x3afa86, 0x3ad868, 0x3ab657, 0x3a9451, 0x3a7259, 0x3a506c,
    0x3a2e8c, 0x3a0cb8, 0x39eaf1, 0x39c936, 0x39a787, 0x3985e4, 0x39644d, 0x3942c3,
    0x392145, 0x38ffd2, 0x38de6c, 0x38bd12, 0x389bc4, 0x387a82, 0x38594c, 0x383822,
    0x381703, 0x37f5f1, 0x37d4eb, 0x37b3f0, 0x379301, 0x37721e, 0x375147, 0x37307c,
    0x370fbc, 0x36ef08, 0x36ce5f, 0x36adc3, 0x368d32, 0x366cac, 0x364c32, 0x362bc4,
    0x360b61, 0x35eb0a, 0x35cabe, 0x35aa7e, 0x358a49, 0x356a1f, 0x354a01, 0x3529ee,
    0x3509e7, 0x34e9eb, 0x34c9fa, 0x34aa15, 0x348a3a, 0x346a6b, 0x344aa7, 0x342aef,
    0x340b41, 0x33eb9f, 0x33cc08, 0x33ac7b, 0x338cfa, 0x336d84, 0x334e19, 0x332eb9,
    0x330f64, 0x32f01a, 0x32d0da, 0x32b1a6, 0x32927d, 0x32735e, 0x32544a, 0x323541,
    0x321643, 0x31f750, 0x31d867, 0x31b989, 0x319ab6, 0x317bee, 0x315d30, 0x313e7d,
    0x311fd4, 0x310136, 0x30e2a3, 0x30c41a, 0x30a59c, 0x308728, 0x3068bf, 0x304a60,
    0x302c0b, 0x300dc2, 0x2fef82, 0x2fd14d, 0x2fb322, 0x2f9502, 0x2f76ec, 0x2f58e0,
    0x2f3ade, 0x2f1ce7, 0x2efefa, 0x2ee117, 0x2ec33f, 0x2ea570, 0x2e87ac, 0x2e69f2,
    0x2e4c42, 0x2e2e9c, 0x2e1100, 0x2df36e, 0x2dd5e7, 0x2db869, 0x2d9af5, 0x2d7d8c,
    0x2d602c, 0x2d42d6, 0x2d258a, 0x2d0848, 0x2ceb10, 0x2ccde2, 0x2cb0bd, 0x2c93a3,
    0x2c7692, 0x2c598b, 0x2c3c8e, 0x2c1f9a, 0x2c02b0, 0x2be5d0, 0x2bc8fa, 0x2bac2d,
    0x2b8f6a, 0x2b72b1, 0x2b5601, 0x2b395b, 0x2b1cbe, 0x2b002b, 0x2ae3a2, 0x2ac722,
    0x2aaaab, 0x2a8e3e, 0x2a71da, 0x2a5580, 0x2a3930, 0x2a1ce8, 0x2a00aa, 0x29e476,
    0x29c84b, 0x29ac29, 0x299010, 0x297401, 0x2957fb, 0x293bfe, 0x29200b, 0x290421,
    0x28e840, 0x28cc68, 0x28b099, 0x2894d4, 0x287917, 0x285d64, 0x2841ba, 0x282619,
    0x280a81, 0x27eef2, 0x27d36c, 0x27b7ef, 0x279c7b, 0x278111, 0x2765af, 0x274a56,
    0x272f06, 0x2713be, 0x26f880, 0x26dd4b, 0x26c21e, 0x26a6fb, 0x268be0, 0x2670ce,
    0x2655c5, 0x263ac4, 0x261fcc, 0x2604dd, 0x25e9f7, 0x25cf1a, 0x25b445, 0x259979,
    0x257eb5, 0x2563fb, 0x254948, 0x252e9f, 0x2513fe, 0x24f965, 0x24ded6, 0x24c44e,
    0x24a9cf, 0x248f59, 0x2474eb, 0x245a86, 0x244029, 0x2425d5, 0x240b89, 0x23f146,
    0x23d70b, 0x23bcd8, 0x23a2ae, 0x23888c, 0x236e72, 0x235461, 0x233a58, 0x232057,
    0x23065f, 0x22ec6e, 0x22d287, 0x22b8a7, 0x229ecf, 0x228500, 0x226b39, 0x22517a,
    0x2237c3, 0x221e15, 0x22046e, 0x21ead0, 0x21d13a, 0x21b7ac, 0x219e26, 0x2184a7,
    0x216b31, 0x2151c3, 0x21385d, 0x211f00, 0x2105a9, 0x20ec5b, 0x20d315, 0x20b9d7,
    0x20a0a1, 0x208773, 0x206e4c, 0x20552e, 0x203c17, 0x202308, 0x200a01, 0x1ff102,
    0x1fd80a, 0x1fbf1b, 0x1fa633, 0x1f8d53, 0x1f747a, 0x1f5baa, 0x1f42e1, 0x1f2a20,
    0x1f1166, 0x1ef8b4, 0x1ee00a, 0x1ec768, 0x1eaecd, 0x1e963a, 0x1e7dae, 0x1e652a,
    0x1e4cad, 0x1e3439, 0x1e1bcb, 0x1e0365, 0x1deb07, 0x1dd2b0, 0x1dba61, 0x1da219,
    0x1d89d9, 0x1d71a0, 0x1d596f, 0x1d4145, 0x1d2922, 0x1d1107, 0x1cf8f3, 0x1ce0e7,
    0x1cc8e2, 0x1cb0e4, 0x1c98ee, 0x1c80ff, 0x1c6917, 0x1c5136, 0x1c395d, 0x1c218b,
    0x1c09c1, 0x1bf1fe, 0x1bda41, 0x1bc28c, 0x1baadf, 0x1b9338, 0x1b7b99, 0x1b6401,
    0x1b4c70, 0x1b34e6, 0x1b1d63, 0x1b05e8, 0x1aee73, 0x1ad706, 0x1abfa0, 0x1aa840,
    0x1a90e8, 0x1a7997, 0x1a624d, 0x1a4b0a, 0x1a33ce, 0x1a1c99, 0x1a056a, 0x19ee43,
    0x19d723, 0x19c00a, 0x19a8f8, 0x1991ec, 0x197ae8, 0x1963ea, 0x194cf3, 0x193604,
    0x191f1b, 0x190838, 0x18f15d, 0x18da89, 0x18c3bb, 0x18acf4, 0x189634, 0x187f7b,
    0x1868c8, 0x18521c, 0x183b77, 0x1824d9, 0x180e42, 0x17f7b1, 0x17e126, 0x17caa3,
    0x17b426, 0x179db0, 0x178740, 0x1770d8, 0x175a75, 0x17441a, 0x172dc5, 0x171776,
    0x17012e, 0x16eaed, 0x16d4b2, 0x16be7e, 0x16a850, 0x169229, 0x167c08, 0x1665ee,
    0x164fdb, 0x1639cd, 0x1623c7, 0x160dc6, 0x15f7cd, 0x15e1d9, 0x15cbec, 0x15b606,
    0x15a026, 0x158a4c, 0x157478, 0x155eab, 0x1548e5, 0x153324, 0x151d6a, 0x1507b7,
    0x14f209, 0x14dc62, 0x14c6c2, 0x14b127, 0x149b93, 0x148605, 0x14707d, 0x145afc,
    0x144581, 0x14300c, 0x141a9d, 0x140534, 0x13efd2, 0x13da76, 0x13c520, 0x13afd0,
    0x139a86, 0x138542, 0x137005, 0x135acd, 0x13459c, 0x133071, 0x131b4c, 0x13062d,
    0x12f114, 0x12dc01, 0x12c6f4, 0x12b1ed, 0x129cec, 0x1287f1, 0x1272fc, 0x125e0e,
    0x124925, 0x123442, 0x121f65, 0x120a8e, 0x11f5bd, 0x11e0f2, 0x11cc2d, 0x11b76d,
    0x11a2b4, 0x118e00, 0x117953, 0x1164ab, 0x115009, 0x113b6d, 0x1126d7, 0x111247,
    0x10fdbc, 0x10e937, 0x10d4b9, 0x10c03f, 0x10abcc, 0x10975f, 0x1082f7, 0x106e95,
    0x105a39, 0x1045e2, 0x103191, 0x101d46, 0x100901, 0x0ff4c1, 0x0fe087, 0x0fcc53,
    0x0fb824, 0x0fa3fb, 0x0f8fd8, 0x0f7bba, 0x0f67a2, 0x0f5390, 0x0f3f83, 0x0f2b7c,
    0x0f177a, 0x0f037e, 0x0eef88, 0x0edb97, 0x0ec7ab, 0x0eb3c6, 0x0e9fe5, 0x0e8c0b,
    0x0e7836, 0x0e6466, 0x0e509c, 0x0e3cd7, 0x0e2918, 0x0e155e, 0x0e01aa, 0x0dedfb,
    0x0dda52, 0x0dc6ae, 0x0db310, 0x0d9f77, 0x0d8be3, 0x0d7855, 0x0d64cc, 0x0d5149,
    0x0d3dcb, 0x0d2a53, 0x0d16df, 0x0d0371, 0x0cf009, 0x0cdca6, 0x0cc948, 0x0cb5ef,
    0x0ca29c, 0x0c8f4e, 0x0c7c06, 0x0c68c2, 0x0c5584, 0x0c424b, 0x0c2f18, 0x0c1bea,
    0x0c08c1, 0x0bf59d, 0x0be27e, 0x0bcf65, 0x0bbc51, 0x0ba942, 0x0b9638, 0x0b8334,
    0x0b7034, 0x0b5d3a, 0x0b4a45, 0x0b3755, 0x0b246b, 0x0b1185, 0x0afea5, 0x0aebc9,
    0x0ad8f3, 0x0ac622, 0x0ab356, 0x0aa08f, 0x0a8dcd, 0x0a7b10, 0x0a6859, 0x0a55a6,
    0x0a42f9, 0x0a3050, 0x0a1dac, 0x0a0b0e, 0x09f874, 0x09e5e0, 0x09d351, 0x09c0c6,
    0x09ae41, 0x099bc0, 0x098945, 0x0976ce, 0x09645c, 0x0951f0, 0x093f88, 0x092d25,
    0x091ac7, 0x09086e, 0x08f61a, 0x08e3cb, 0x08d181, 0x08bf3b, 0x08acfb, 0x089abf,
    0x088889, 0x087657, 0x08642a, 0x085201, 0x083fde, 0x082dbf, 0x081ba6, 0x080991,
    0x07f781, 0x07e575, 0x07d36f, 0x07c16d, 0x07af70, 0x079d78, 0x078b84, 0x077995,
    0x0767ab, 0x0755c6, 0x0743e6, 0x07320a, 0x072033, 0x070e60, 0x06fc93, 0x06eaca,
    0x06d905, 0x06c746, 0x06b58b, 0x06a3d4, 0x069223, 0x068076, 0x066ecd, 0x065d2a,
    0x064b8b, 0x0639f0, 0x06285a, 0x0616c9, 0x06053c, 0x05f3b4, 0x05e231, 0x05d0b2,
    0x05bf37, 0x05adc2, 0x059c50, 0x058ae4, 0x05797c, 0x056818, 0x0556b9, 0x05455e,
    0x053408, 0x0522b7, 0x05116a, 0x050021, 0x04eedd, 0x04dd9e, 0x04cc63, 0x04bb2c,
    0x04a9fa, 0x0498cc, 0x0487a3, 0x04767e, 0x04655e, 0x045442, 0x04432a, 0x043217,
    0x042108, 0x040ffe, 0x03fef8, 0x03edf6, 0x03dcf9, 0x03cc01, 0x03bb0c, 0x03aa1c,
    0x039930, 0x038849, 0x037766, 0x036687, 0x0355ad, 0x0344d7, 0x033405, 0x032338,
    0x03126f, 0x0301aa, 0x02f0e9, 0x02e02d, 0x02cf75, 0x02bec1, 0x02ae12, 0x029d67,
    0x028cc0, 0x027c1d, 0x026b7f, 0x025ae4, 0x024a4e, 0x0239bd, 0x02292f, 0x0218a6,
    0x020821, 0x01f7a0, 0x01e723, 0x01d6aa, 0x01c636, 0x01b5c5, 0x01a559, 0x0194f1,
    0x01848e, 0x01742e, 0x0163d3, 0x01537b, 0x014328, 0x0132d9, 0x01228e, 0x011247,
    0x010204, 0x00f1c5, 0x00e18b, 0x00d154, 0x00c122, 0x00b0f3, 0x00a0c9, 0x0090a3,
    0x008081, 0x007062, 0x006048, 0x005032, 0x004020, 0x003012, 0x002008, 0x001002,
    0x000000,
];

fn sar(x: u32, n: u32) -> u32 {
    ((x as i32) >> (n & 31)) as u32
}

/// Multiplies (FUN_6f06f9c0): the mantissas' product truncated.
pub fn mul(a: u32, b: u32) -> u32 {
    let sign = (a ^ b) & 0x8000_0000;
    let (ea, eb) = (a & 0x7f80_0000, b & 0x7f80_0000);
    let (ma, mb) = (a & 0x7f_ffff, b & 0x7f_ffff);
    if ma != 0 && mb != 0 {
        let l = (((mb | 0xff80_0000) << 8) as u64) * (((ma | 0xff80_0000) << 8) as u64);
        let hi = (l >> 32) as u32;
        let top = (l >> 63) as u32;
        let e = ea.wrapping_add(0xc080_0000).wrapping_add(eb);
        let carry: u32 = if hi & 0x8000_0000 != 0 { 0x80_0000 } else { 0 };
        let r = ((hi >> top) >> 7 & 0x7f_ffff) | carry.wrapping_add(e) | sign;
        return r & !sar(e.wrapping_sub(0x80_0000), 31);
    }
    if ea != 0 && eb != 0 {
        let e = ea.wrapping_add(eb).wrapping_add(0xc080_0000);
        return (e | ma | mb | sign) & !sar(e.wrapping_sub(0x80_0000), 31);
    }
    0
}

fn add_bits(a: u32, b: u32) -> u32 {
    let ea = a & 0x7f80_0000;
    if ea == 0 {
        return b;
    }
    let eb = b & 0x7f80_0000;
    if eb == 0 {
        return a;
    }
    let signed = |x: u32| ((((x & 0x7f_ffff) | 0x80_0000) * 2) ^ sar(x, 31)).wrapping_sub(sar(x, 31)) as i32;
    let (mut ma, mut mb) = (signed(a), signed(b));
    let d = eb.wrapping_sub(ea) as i32;
    let mut e = ea;
    if d < 1 {
        if d < -0xb7f_ffff {
            return a;
        }
        mb >>= (ea.wrapping_sub(eb) >> 23) & 31;
    } else {
        if d > 0xb7f_ffff {
            return b;
        }
        ma >>= (d as u32 >> 23) & 31;
        e = eb;
    }
    let s = mb.wrapping_add(ma) as u32;
    if s == 0 {
        return 0;
    }
    let mag = (s ^ sar(s, 31)).wrapping_sub(sar(s, 31));
    let z = mag.leading_zeros() as i32;
    let k = 8 - z;
    let v = if k < 0 { mag << (-k) } else { mag >> k };
    ((7 - z) as u32).wrapping_mul(0x80_0000).wrapping_add(e) | (v & 0x7f_ffff) | (s & 0x8000_0000)
}

/// Adds (FUN_6f06fbb0): the smaller aligned by shifting, its lost bits dropped.
pub fn add(a: u32, b: u32) -> u32 {
    add_bits(a, b)
}

/// Subtracts (FUN_6f06fa90).
pub fn sub(a: u32, b: u32) -> u32 {
    add_bits(a, b ^ 0x8000_0000)
}

/// The reciprocal (FUN_6f0711e0), interpolated in the table.
pub fn recip(b: u32) -> u32 {
    let t = &RECIPROCAL;
    let i = ((b & 0x7f_ffff) >> 13) as usize;
    let (t0, t1) = (t[i], t[i + 1]);
    let x = ((((b & 0x1fff) << 6) | (b << 19)) >> 13) | (b << 19);
    let i3 = t0.wrapping_sub(((t0.wrapping_sub(t1) as u64 * x as u64) >> 32) as u32).wrapping_sub(b & 0x7f80_0000);
    !sar(i3.wrapping_add(0x7e00_0000), 31) & ((b & 0x8000_0000) | i3.wrapping_add(0x7e80_0000))
}

/// Divides (FUN_6f06fcd0): by multiplying by the reciprocal; a number by itself is 1.
pub fn div(a: u32, b: u32) -> u32 {
    if a == b {
        return 0x3f80_0000;
    }
    mul(a, recip(b))
}

/// An integer as a real (FUN_6f070d80), its low bits dropped.
pub fn itof(n: i32) -> u32 {
    let n = n as u32;
    if n == 0 {
        return 0;
    }
    let a = (n ^ sar(n, 31)).wrapping_sub(sar(n, 31));
    let z = a.leading_zeros() as i32;
    let k = z - 8;
    let v = if k < 0 { a >> (-k) } else { a << k };
    ((0x9e - z) as u32).wrapping_mul(0x80_0000) | (v & 0x7f_ffff) | (n & 0x8000_0000)
}

/// A real as an integer, toward zero (FUN_6f070120).
pub fn ftoi(b: u32) -> i32 {
    let e = (b >> 23) & 0xff;
    if e < 0x7f {
        return 0;
    }
    let m = (b & 0x7f_ffff) | 0x80_0000;
    let k = 0x96 - e as i32;
    let s = sar(b, 31);
    let v = if k >= 0 { m.checked_shr(k as u32).unwrap_or(0) } else { m.checked_shl((-k) as u32).unwrap_or(0) };
    (s ^ v).wrapping_sub(s) as i32
}

/// Rounds down (FUN_6f070c80).
pub fn floor(b: u32) -> u32 {
    let e = ((b >> 23) & 0xff) as i32 - 0x7f;
    if e < 0 {
        return if (b << 1) & sar(b, 31) != 0 { 0xbf80_0000 } else { 0 };
    }
    if e < 23 {
        let mut m = sar(0x8000_0000, ((b >> 23).wrapping_add(0x89)) & 31);
        if (b << 1) & !sar(b, 31) != 0 {
            return b & m;
        }
        m &= (b & 0x7f_ffff).wrapping_add(!m);
        return (((m >> 23) as i32 + 0x7f + e) as u32).wrapping_mul(0x80_0000) | (m & 0x7f_ffff) | 0x8000_0000;
    }
    b
}

/// What lies beyond the whole part (FUN_6f070d20).
fn fraction(b: u32) -> u32 {
    let e = ((b >> 23) & 0xff) as i32 - 0x7f;
    if e < 0 {
        return b;
    }
    if e < 23 {
        return sub(b, sar(0x8000_0000, (e + 8) as u32) & b);
    }
    0
}

/// Ten to a power (FUN_6f071180).
fn pow10(mut n: i32) -> u32 {
    let (mut r, mut x) = (0x3f80_0000, 10f32.to_bits());
    while n != 0 {
        if n & 1 != 0 {
            r = mul(r, x);
        }
        x = mul(x, x);
        n >>= 1;
    }
    r
}

/// Reads a real as S2R does (FUN_6f070de0): a sign, then digits and one point up to a
/// space, semicolon, tab, or anything else; nine significant digits, the rest only
/// scaling the whole part.
pub fn parse(s: &str) -> u32 {
    let b = s.as_bytes();
    let (mut i, mut sign) = (0, 1i32);
    match b.first() {
        Some(b'-') => (i, sign) = (1, -1),
        Some(b'+') => i = 1,
        _ => {}
    }
    let (mut mant, mut sig, mut started, mut scale, mut dot, mut step) = (0i32, 0, false, 0i32, false, 0i32);
    while let Some(&c) = b.get(i) {
        i += 1;
        if c == b' ' || c == b';' || c == b'\t' {
            break;
        }
        if c.is_ascii_digit() {
            started = started || c != b'0';
            sig += started as i32;
            if sig <= 9 {
                scale += step;
                mant = mant.wrapping_mul(10).wrapping_add((c - b'0') as i32);
            } else {
                if sig == 10 {
                    step -= 1;
                }
                scale += step;
            }
        } else if c == b'.' && !dot {
            dot = true;
            step += 1;
        } else {
            break;
        }
    }
    let v = itof(mant.wrapping_mul(sign));
    if scale >= 0 { div(v, pow10(scale)) } else { mul(v, pow10(-scale)) }
}

/// A real literal in a script: its whole part as a wrapping integer, plus its fraction
/// as S2R reads it (measured: 3.14159 is 0x40490fcf, where S2R reads 0x40490fd1).
pub fn literal(whole: i32, frac: &str) -> u32 {
    let w = itof(whole);
    if frac.is_empty() { w } else { add(w, parse(&format!("0.{frac}"))) }
}

/// Prints a real as R2SW does (FUN_6f0701d0): a minus sign apart; the whole part padded
/// to the width less the precision (at least 1); the fraction to the precision (at most
/// 9; a negative one is 6 with trailing zeros dropped down to one); and whole parts of
/// 2^31 or more as their leading digits and zeros.
pub fn format(b: u32, width: i32, precision: i32) -> String {
    let mut out = String::new();
    if (b << 1) & sar(b, 31) != 0 {
        out.push('-');
    }
    let a = b & 0x7fff_ffff;
    if a & 0x7f80_0000 >= 0x4f00_0000 {
        if f32::from_bits(a) < f32::MAX {
            let mut n = 0;
            let mut x = a;
            loop {
                n += 1;
                x = div(x, 10f32.to_bits());
                if x < 0x4f00_0000 {
                    break;
                }
            }
            out += &format!("{}{}.{}", ftoi(x), "0".repeat(n), "0".repeat(precision.max(1) as usize));
            return out;
        }
        out += "inf";
        return out;
    }
    let trim = precision < 0;
    let mut p = if trim { 6 } else { precision.min(9) };
    let mut whole = ftoi(a);
    let pw = 10i32.pow(p as u32);
    let mut f = ftoi(floor(add(mul(fraction(a), itof(pw)), 0.5f32.to_bits())));
    if pw <= f {
        whole += 1;
        f -= pw;
    }
    if trim {
        while p > 1 && f % 10 == 0 {
            p -= 1;
            f /= 10;
        }
    }
    let w = (width - p).max(1) as usize;
    out += &format!("{whole:>w$}.{f:0p$}", p = p as usize);
    out
}

const ONE: u32 = 0x3f80_0000;
const HALF_PI: u32 = 0x3fc9_0fdb;
const PI: u32 = 0x4049_0fdb;

/// sin(i/1024 of a right angle) in 31 bits, toward zero (game.dll's table at 0x6fa820c8).
fn sine_table() -> &'static [i32; 1025] {
    static T: OnceLock<Box<[i32; 1025]>> = OnceLock::new();
    T.get_or_init(|| {
        let mut t = Box::new([0i32; 1025]);
        for (i, v) in t.iter_mut().enumerate() {
            *v = ((i as f64 * std::f64::consts::PI / 2048.0).sin() * 2f64.powi(31)).floor().min(i32::MAX as f64) as i32;
        }
        t
    })
}

/// Sine or cosine (FUN_6f071280, FUN_6f070790): the turn in 2^20 parts, read from the
/// quarter-wave table between neighbors.
fn sin_cos(x: u32, cos: bool) -> u32 {
    let t = sine_table();
    let n = ftoi(mul(x, 0x4822_f983));
    let q = ((n >> 18) + cos as i32) & 3;
    let i = ((n >> 8) & 0x3ff) as usize;
    let f = (n & 0xff) as u32 * 0x0101_0101;
    let r = if q & 1 == 0 {
        let (a, b) = (t[i], t[i + 1]);
        a.wrapping_add(((b.wrapping_sub(a) as u32 as u64 * f as u64) >> 32) as i32)
    } else {
        let (a, c) = (t[1024 - i], t[1023 - i]);
        a.wrapping_sub(((a.wrapping_sub(c) as u32 as u64 * f as u64) >> 32) as i32)
    };
    let r = if q & 2 != 0 { r.wrapping_neg() } else { r };
    let v = itof(r);
    if v & 0x7f80_0000 != 0 { v.wrapping_sub(0x0f80_0000) } else { v }
}

pub fn sin(x: u32) -> u32 {
    sin_cos(x, false)
}

pub fn cos(x: u32) -> u32 {
    sin_cos(x, true)
}

/// Tangent: the sine over the cosine (FUN_6f071590).
pub fn tan(x: u32) -> u32 {
    div(sin(x), cos(x))
}

/// The integer square root of 32 bits (FUN_6f071530).
fn isqrt(mut p: u32) -> u32 {
    let (mut rem, mut root) = (0u32, 0u32);
    let mut two = 0;
    for _ in 0..16 {
        two = root.wrapping_mul(2);
        rem = rem.wrapping_mul(4) | (p >> 30);
        p <<= 2;
        if two < rem {
            rem = rem.wrapping_sub(root.wrapping_mul(2)).wrapping_sub(1);
            two = two.wrapping_add(2);
        }
        root = two;
    }
    two >> 1
}

/// Square root (FUN_6f071480): of the mantissa by integers, scaled, times half the
/// exponent's power of two (and the square root of 2 for an odd exponent).
pub fn sqrt(b: u32) -> u32 {
    if (b << 1) & !sar(b, 31) == 0 {
        return 0;
    }
    let m = b & 0x7f_ffff;
    let e = ((b >> 23) & 0xff) as i32 - 0x7f;
    let s = isqrt(((m | 0xff80_0000) << 8) | (m >> 15));
    let a = (s.wrapping_sub(0xb504).wrapping_mul(0xb505) >> 8) | 0x3f80_0000;
    let odd = -(e & 1);
    let half = (e + odd) / 2;
    let p = (((half + 0x7f) as u32) << 23) | (odd as u32 & 0x35_04f3);
    mul(p, a)
}

/// Arctangent (FUN_6f0705b0): beyond 1 by its reciprocal, beyond tan 15 degrees shifted
/// by 30, then a ratio of polynomials.
pub fn atan(x: u32) -> u32 {
    let gt = |a: u32, b: u32| f32::from_bits(a) > f32::from_bits(b);
    let mut a = x & 0x7fff_ffff;
    let inv = gt(a, ONE);
    if inv {
        a = recip(a);
    }
    let big = gt(a, 0x3e89_30a3);
    if big {
        a = div(add(a, 0xbf13_cd3a), add(ONE, mul(0x3f13_cd3a, a)));
    }
    let s = mul(a, a);
    let mut r = div(mul(a, add(0x3f7f_fff0, mul(0x3e84_15a6, s))), add(ONE, mul(0x3f17_592e, s)));
    if big {
        r = add(r, 0x3f06_0a92);
    }
    if inv {
        r = sub(HALF_PI, r);
    }
    if (x << 1) & sar(x, 31) != 0 {
        r ^= 0x8000_0000;
    }
    r
}

/// The angle of (x, y) (FUN_6f070530).
pub fn atan2(y: u32, x: u32) -> u32 {
    let mut r = if x & 0x7f80_0000 != 0 { atan(div(y, x) & 0x7fff_ffff) } else { HALF_PI };
    if (x << 1) & sar(x, 31) != 0 {
        r = sub(PI, r);
    }
    if (y << 1) & sar(y, 31) != 0 {
        r ^= 0x8000_0000;
    }
    r
}

/// acos(i/1024) in 29 bits, toward zero, its last two entries as game.dll has them
/// (its table at 0x6fa830d0).
fn acos_table() -> &'static [i32; 1026] {
    static T: OnceLock<Box<[i32; 1026]>> = OnceLock::new();
    T.get_or_init(|| {
        let mut t = Box::new([0i32; 1026]);
        for (i, v) in t.iter_mut().enumerate().take(1024) {
            *v = ((i as f64 / 1024.0).acos() * 2f64.powi(29)).floor() as i32;
        }
        t[1024] = 1 << 20;
        t
    })
}

/// acos near 1 in 34 bits (game.dll's table at 0x6fa840d8): eight entries an octave of
/// the float steps below 1, as 32-bit floats; its last sixteen as game.dll has them.
fn near_one_table() -> &'static [i32; 137] {
    static T: OnceLock<Box<[i32; 137]>> = OnceLock::new();
    T.get_or_init(|| {
        let mut t = Box::new([0i32; 137]);
        for (k, v) in t.iter_mut().enumerate().take(121) {
            let (z, top) = (15 + k / 8, (k % 8) as f64);
            let steps = 2f64.powi(31 - z as i32) * (1.0 + (8.0 - top) / 8.0);
            *v = (((1.0 - steps / 16_777_216.0).acos() * 2f64.powi(34)) as f32 as f64).min(i32::MAX as f64) as i32;
        }
        let tail = [11_863_283; 3].into_iter().chain([8_388_608; 4]).chain([5_931_641; 8]);
        for (v, x) in t[121..136].iter_mut().zip(tail) {
            *v = x;
        }
        t
    })
}

/// Arcsine or arccosine up to 0.994 or so (FUN_6f0703a0, FUN_6f06ffa0): from the table
/// between neighbors, the arcsine as the arccosine of the negation less a right angle.
fn acos_core(x: u32, asin: bool) -> u32 {
    let t = acos_table();
    let mut n = ftoi(x.wrapping_add(0x0f00_0000));
    let e = (n as u32).wrapping_add(0x3fff_ffff);
    if e > 0x7fff_fffe {
        n = ((!sar(e, 31) & 0x7fff_fffe).wrapping_sub(0x3fff_ffff)) as i32;
    }
    if asin {
        n = n.wrapping_neg();
    }
    let f = ((n as u32) << 12) | (((n as u32) << 12) >> 20);
    let i = ((n >> 20) & 0x3ff) as usize;
    let (d, mut c) = if n >= 0 {
        (t[i].wrapping_sub(t[i + 1]) as u32, t[i] as u32)
    } else {
        let a = t[1024 - i];
        (t[1023 - i].wrapping_sub(a) as u32, 0x6487_ed51u32.wrapping_sub(a as u32))
    };
    if asin {
        c = c.wrapping_add(0xcdbc_0958);
    }
    let r = c.wrapping_sub(((d as u64 * f as u64) >> 32) as u32) as i32;
    let v = itof(r);
    if v & 0x7f80_0000 != 0 { v.wrapping_sub(0x0e80_0000) } else { v }
}

/// The arccosine of a magnitude near 1, by the float steps it lies below 1.
fn acos_near_one(x: u32) -> u32 {
    let t = near_one_table();
    let mut a = x & 0x7fff_ffff;
    if a > 0x3f80_0000 {
        a = !sar(a, 31) & 0x3f80_0000;
    }
    let diff = 0x3f80_0000 - a;
    let z = diff.leading_zeros();
    let m = if z < 32 { !diff << z } else { 0 };
    let k = ((z * 8).wrapping_sub(0x78) | (m >> 28)) as usize;
    let l0 = t[k];
    let d = l0.wrapping_sub(t.get(k + 1).copied().unwrap_or(0)) as u32;
    let r = l0.wrapping_sub(((d as u64 * (m << 4) as u64) >> 32) as i32);
    let v = itof(r);
    if v & 0x7f80_0000 != 0 { v.wrapping_sub(0x1100_0000) } else { v }
}

/// Arccosine (FUN_6f06ffa0).
pub fn acos(x: u32) -> u32 {
    if x & 0x7fff_ffff <= 0x3f7e_8000 {
        return acos_core(x, false);
    }
    let r = acos_near_one(x);
    if x & 0x8000_0000 != 0 { sub(PI, r) } else { r }
}

/// Arcsine (FUN_6f0703a0).
pub fn asin(x: u32) -> u32 {
    if x & 0x7fff_ffff <= 0x3f7e_8000 {
        return acos_core(x, true);
    }
    let r = acos_near_one(x);
    if x & 0x8000_0000 != 0 { add(HALF_PI ^ 0x8000_0000, r) } else { sub(HALF_PI, r) }
}

/// The whole part as a real (FUN_6f0715c0).
fn trunc(b: u32) -> u32 {
    let e = ((b >> 23) & 0xff) as i32 - 0x7f;
    if e < 0 {
        0
    } else if e < 23 {
        sar(0x8000_0000, (e + 8) as u32) & b
    } else {
        b
    }
}

/// A power by repeated squaring (FUN_6f071180).
fn powi(base: u32, mut n: i32) -> u32 {
    let (mut r, mut x) = (ONE, base);
    while n != 0 {
        if n & 1 != 0 {
            r = mul(r, x);
        }
        x = mul(x, x);
        n >>= 1;
    }
    r
}

/// The natural logarithm near 1 (FUN_6f06fd50): from (x - 1) / (x + 1) by a ratio of
/// polynomials.
fn ln_core(x: u32) -> u32 {
    let q = div(add(x, 0xbf80_0000), add(x, ONE));
    let s = mul(q, q);
    let mut u = mul(q, add(ONE, mul(0xbe88_d424, s)));
    if u & 0x7f80_0000 != 0 {
        u = u.wrapping_add(0x80_0000);
    }
    div(u, add(ONE, mul(0xbf19_bf59, s)))
}

/// The natural logarithm of a mantissa (FUN_6f06ff20), scaled toward 1 when small.
fn ln_mantissa(m: u32) -> u32 {
    let f = f32::from_bits(m);
    if f >= f32::from_bits(0x3f61_2ad1) {
        ln_core(m)
    } else if f >= f32::from_bits(0x3f28_e5a3) {
        add(ln_core(mul(m, 0x3fa8_e5a3)), 0xbe8d_f4e0)
    } else {
        add(ln_core(mul(m, 0x3fde_dc67)), 0xbf0d_f4e0)
    }
}

/// The natural logarithm of a magnitude (FUN_6f070f70): its mantissa's in base 2 plus its
/// exponent, times ln 2.
fn ln(x: u32) -> u32 {
    let t = mul(ln_mantissa((x & 0x7f_ffff) | ONE), 0x3fb8_aa3b);
    mul(0x3f31_7218, add(itof(((x >> 23) & 0xff) as i32 - 0x7f), t))
}

/// e to a power of 0 or more (FUN_6f06fe10): e^(1/4) to the quarters, times a polynomial
/// in what is left.
fn exp_core(x: u32) -> u32 {
    let y = x.wrapping_add(if x & 0x7f80_0000 != 0 { 0x100_0000 } else { 0 });
    let whole = trunc(y);
    let f = sub(y, whole);
    let g = !sar(f.wrapping_sub(0x180_0000) ^ f, 31) & f.wrapping_add(0xff00_0000);
    let mut p = mul(0x3c1a_534c, g);
    for c in [0x3d29_6ec9, 0x3e2a_b479, 0x3eff_ffbd, ONE] {
        p = mul(add(p, c), g);
    }
    mul(add(p, ONE), powi(0x3fa4_5af2, ftoi(whole)))
}

/// e to a power (FUN_6f070c20); of a negative one, the reciprocal.
fn exp(x: u32) -> u32 {
    if (x << 1) & sar(x, 31) != 0 { recip(exp_core(x ^ 0x8000_0000)) } else { exp_core(x) }
}

/// A power (FUN_6f0710e0): to a whole power of 0 or more by multiplying; otherwise of the
/// base's magnitude, by its logarithm.
pub fn pow(b: u32, e: u32) -> u32 {
    if f32::from_bits(trunc(e)) == f32::from_bits(e) && (e << 1) & sar(e, 31) == 0 {
        return powi(b, ftoi(e));
    }
    if b & 0x7f80_0000 == 0 {
        return 0;
    }
    exp(mul(e, ln(b)))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Measured in the original.
    #[test]
    fn matches_the_original() {
        assert_eq!(format(parse("0.9995"), 0, 3), "1.000");
        assert_eq!(format(literal(0, "0025"), 0, 3), "0.002");
        assert_eq!(literal(3, "14159"), 0x4049_0fcf);
        assert_eq!(parse("3.14159"), 0x4049_0fd1);
        assert_eq!(format(mul(literal(1, "1"), literal(1, "1")), 1, 9), "1.209999680");
        assert_eq!(format(mul(literal(0, "1"), itof(3)), 1, 9), "0.300000000");
        assert_eq!(format(literal(3, "14159"), 10, 2), "       3.14");
        assert_eq!(format(literal(3, "14159") ^ 0x8000_0000, 2, 0), "- 3.0");
        assert_eq!(format(0.5f32.to_bits(), -3, -1), "0.5");
        assert_eq!(parse(" 2.5"), 0);
        assert_eq!(format(itof(123_456_789), 0, 3), "123456784.000");
        let p = |b: u32| format(b, 1, 9);
        let l = |w: i32, f: &str| literal(w, f);
        assert_eq!([p(sin(l(1, "0"))), p(sin(l(3, "0"))), p(sin(l(2, "0") ^ 0x8000_0000)), p(sin(l(10, "0")))], ["0.841473600", "0.141116768", "-0.909297984", "-0.544023296"]);
        assert_eq!([p(cos(l(1, "0"))), p(cos(l(0, "5"))), p(cos(l(100, "0")))], ["0.540297920", "0.877579776", "0.862317184"]);
        assert_eq!([p(tan(l(0, "5"))), p(tan(l(1, "0")))], ["0.546309760", "1.557425600"]);
        assert_eq!([p(sqrt(l(2, "0"))), p(sqrt(l(1000, "0"))), p(sqrt(l(0, "3")))], ["1.414213536", "31.623027776", "0.547722368"]);
        assert_eq!([p(pow(l(10, "0"), l(2, "5"))), p(pow(l(2, "0"), l(0, "5"))), p(pow(l(1, "5"), l(3, "0")))], ["316.227294912", "1.414213536", "3.375000000"]);
        let pairs = |v: u32| [p(asin(v)), p(acos(v)), p(asin(v ^ 0x8000_0000)), p(acos(v ^ 0x8000_0000))];
        assert_eq!(pairs(l(0, "3")), ["0.304692672", "1.266103616", "-0.304692672", "1.875488960"]);
        assert_eq!(pairs(l(0, "995")), ["1.470775232", "0.100021064", "-1.470775232", "3.041571616"]);
        assert_eq!(pairs(l(0, "9999")), ["1.556653824", "0.014142452", "-1.556653952", "3.127450224"]);
        assert_eq!(pairs(l(1, "0")), ["1.570796352", "0.000000000", "-1.570796352", "3.141592736"]);
        assert_eq!(pairs(l(0, "5")), ["0.523598720", "1.047197460", "-0.523598720", "2.094394920"]);
        assert_eq!([p(atan(l(0, "5"))), p(atan(l(3, "0"))), p(atan2(l(1, "0"), l(2, "0"))), p(atan2(l(3, "0") ^ 0x8000_0000, l(1, "0") ^ 0x8000_0000))], ["0.463647616", "1.249045840", "0.463647616", "-1.892546880"]);
    }
}
