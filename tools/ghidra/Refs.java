// Lists the functions referencing each address argument, and decompiles them.
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.symbol.*;
import java.util.*;

public class Refs extends GhidraScript {
    @Override
    public void run() throws Exception {
        DecompInterface di = new DecompInterface();
        di.openProgram(currentProgram);
        Set<Function> seen = new LinkedHashSet<>();
        for (String s : getScriptArgs()) {
            for (Reference r : getReferencesTo(toAddr(s))) {
                Function f = getFunctionContaining(r.getFromAddress());
                println("ref " + s + " from " + r.getFromAddress() + " in " + (f == null ? "?" : f.getName()));
                if (f != null) seen.add(f);
            }
        }
        for (Function f : seen) {
            DecompileResults r = di.decompileFunction(f, 120, monitor);
            println("==== " + f.getName() + "\n" + (r.decompileCompleted() ? r.getDecompiledFunction().getC() : "(failed)"));
        }
    }
}
