// Prints, for each string argument, where it occurs, the functions referencing it, and
// their decompilation.  Arguments starting with "fn:" decompile that function address.
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.program.model.address.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.mem.*;
import ghidra.program.model.symbol.*;
import java.util.*;

public class StrRefs extends GhidraScript {
    DecompInterface di;

    void decomp(Function f) {
        DecompileResults r = di.decompileFunction(f, 120, monitor);
        println("==== " + f.getName() + " @ " + f.getEntryPoint());
        println(r.decompileCompleted() ? r.getDecompiledFunction().getC() : "(failed: " + r.getErrorMessage() + ")");
    }

    @Override
    public void run() throws Exception {
        di = new DecompInterface();
        di.openProgram(currentProgram);
        Memory mem = currentProgram.getMemory();
        Listing listing = currentProgram.getListing();
        Set<Function> seen = new HashSet<>();
        for (String s : getScriptArgs()) {
            if (s.startsWith("asm:")) {
                Function f = getFunctionContaining(toAddr(s.substring(4)));
                println("==== asm " + f.getName());
                for (Instruction i : listing.getInstructions(f.getBody(), true)) println(i.getAddress() + "  " + i);
                continue;
            }
            if (s.startsWith("callers:")) {
                Function f = getFunctionContaining(toAddr(s.substring(8)));
                for (Reference ref : getReferencesTo(f.getEntryPoint())) {
                    Function c = getFunctionContaining(ref.getFromAddress());
                    println("   caller " + ref.getFromAddress() + " " + ref.getReferenceType() + " in " + (c == null ? "?" : c.getName()));
                }
                continue;
            }
            if (s.startsWith("fn:")) {
                Function f = getFunctionAt(toAddr(s.substring(3)));
                if (f == null) f = getFunctionContaining(toAddr(s.substring(3)));
                if (f != null && seen.add(f)) decomp(f);
                continue;
            }
            byte[] pat = (s + "\0").getBytes("ISO-8859-1");
            Address a = mem.getMinAddress();
            while (true) {
                a = mem.findBytes(a, pat, null, true, monitor);
                if (a == null) break;
                println("## \"" + s + "\" at " + a);
                for (Reference ref : getReferencesTo(a)) {
                    Function f = getFunctionContaining(ref.getFromAddress());
                    println("   ref from " + ref.getFromAddress() + " in " + (f == null ? "?" : f.getName()));
                    if (f != null && seen.add(f)) decomp(f);
                }
                a = a.add(1);
            }
        }
    }
}
