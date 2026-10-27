// Decompiles every function whose entry lies in [arg0, arg1) and prints those whose C
// contains arg2 (or all, without arg2).
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.program.model.listing.*;

public class Range extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] a = getScriptArgs();
        DecompInterface di = new DecompInterface();
        di.openProgram(currentProgram);
        for (Function f : currentProgram.getFunctionManager().getFunctions(toAddr(a[0]), true)) {
            if (f.getEntryPoint().compareTo(toAddr(a[1])) >= 0) break;
            DecompileResults r = di.decompileFunction(f, 60, monitor);
            if (!r.decompileCompleted()) continue;
            String c = r.getDecompiledFunction().getC();
            if (a.length < 3 || c.contains(a[2])) println("==== " + f.getName() + "\n" + c);
        }
    }
}
