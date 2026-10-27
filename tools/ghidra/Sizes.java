// Prints each function whose entry lies in [arg0, arg1) with its body's size in bytes,
// largest first.
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import java.util.*;

public class Sizes extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] a = getScriptArgs();
        List<Function> fs = new ArrayList<>();
        for (Function f : currentProgram.getFunctionManager().getFunctions(toAddr(a[0]), true)) {
            if (f.getEntryPoint().compareTo(toAddr(a[1])) >= 0) break;
            fs.add(f);
        }
        fs.sort((x, y) -> Long.compare(y.getBody().getNumAddresses(), x.getBody().getNumAddresses()));
        for (Function f : fs) println(f.getEntryPoint() + " " + f.getBody().getNumAddresses());
    }
}
