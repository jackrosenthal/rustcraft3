// For each address argument: the function containing it, decompiled.
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.program.model.listing.*;

public class At extends GhidraScript {
    @Override
    public void run() throws Exception {
        DecompInterface di = new DecompInterface();
        di.openProgram(currentProgram);
        for (String s : getScriptArgs()) {
            Function f = getFunctionContaining(toAddr(s));
            if (f == null) { println("==== " + s + " in no function"); continue; }
            DecompileResults r = di.decompileFunction(f, 120, monitor);
            println("==== " + s + " in " + f.getName() + " @ " + f.getEntryPoint());
            println(r.decompileCompleted() ? r.getDecompiledFunction().getC() : "(failed)");
        }
    }
}
