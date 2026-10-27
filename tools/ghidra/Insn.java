// Prints each instruction in [arg0, arg1) whose text matches the regular expression
// arg2, with the function holding it.
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import java.util.regex.*;

public class Insn extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] a = getScriptArgs();
        Pattern p = Pattern.compile(a[2]);
        Listing l = currentProgram.getListing();
        for (Instruction i : l.getInstructions(toAddr(a[0]), true)) {
            if (i.getAddress().compareTo(toAddr(a[1])) >= 0) break;
            String t = i.toString();
            if (p.matcher(t).find()) {
                Function f = getFunctionContaining(i.getAddress());
                println(i.getAddress() + "  " + t + "  in " + (f == null ? "?" : f.getName()));
            }
        }
    }
}
