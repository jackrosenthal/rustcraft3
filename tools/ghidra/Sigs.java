// Gives the engine's float routines their signatures (fastcall: the result's address
// in ECX, the first operand's in EDX, the second on the stack), then decompiles each
// function whose address is an argument, so their arithmetic reads as calls.  Nothing
// is saved.
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.program.model.data.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.symbol.SourceType;

public class Sigs extends GhidraScript {
    void sig(String addr, String name, int params, boolean returnsInt) throws Exception {
        Function f = getFunctionAt(toAddr(addr));
        if (f == null) return;
        f.setName(name, SourceType.USER_DEFINED);
        DataType u = new PointerDataType(UnsignedIntegerDataType.dataType);
        java.util.List<ParameterImpl> ps = new java.util.ArrayList<>();
        String[] names = {"out", "a", "b"};
        for (int i = 0; i < params; i++) ps.add(new ParameterImpl(names[i], u, currentProgram));
        f.setCallingConvention("__fastcall");
        f.replaceParameters(ps, Function.FunctionUpdateType.DYNAMIC_STORAGE_ALL_PARAMS, true, SourceType.USER_DEFINED);
        f.setReturnType(returnsInt ? IntegerDataType.dataType : u, SourceType.USER_DEFINED);
    }

    @Override
    public void run() throws Exception {
        sig("6f06fbb0", "fadd", 3, false);
        sig("6f06fa90", "fsub", 3, false);
        sig("6f06f9c0", "fmul", 3, false);
        sig("6f06fcd0", "fdiv", 3, false);
        sig("6f071480", "fsqrt", 2, false);
        sig("6f0711e0", "frecip", 2, false);
        sig("6f070c80", "ffloor", 2, false);
        sig("6f070d80", "itof", 2, false);
        DecompInterface di = new DecompInterface();
        di.openProgram(currentProgram);
        for (String a : getScriptArgs()) {
            Function f = getFunctionContaining(toAddr(a));
            if (f == null) continue;
            DecompileResults r = di.decompileFunction(f, 120, monitor);
            println("==== " + f.getName() + " @ " + f.getEntryPoint());
            println(r.decompileCompleted() ? r.getDecompiledFunction().getC() : "(failed)");
        }
    }
}
