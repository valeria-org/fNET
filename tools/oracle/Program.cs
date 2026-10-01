// Independent development oracle. Normal Cargo builds/tests do not need .NET.
// Generate checked-in opcode metadata and decoded real CoreLib method samples.
using System.Reflection;
using System.Reflection.Emit;
using System.Reflection.Metadata;
using System.Reflection.PortableExecutable;

if (args.Length < 1) throw new ArgumentException("Usage: oracle <output-directory> [assembly-path] [maximum-methods]");
var output = Path.GetFullPath(args[0]);
Directory.CreateDirectory(output);
var opcodes = typeof(OpCodes).GetFields(BindingFlags.Public | BindingFlags.Static)
    .Where(f => f.FieldType == typeof(OpCode)).Select(f => (OpCode)f.GetValue(null)!)
    .OrderBy(o => (ushort)o.Value).ToArray();
var lookup = opcodes.ToDictionary(o => (ushort)o.Value);
File.WriteAllLines(Path.Combine(output, "reflection-opcodes.tsv"), opcodes.Select(o =>
    $"{(ushort)o.Value:X4}\t{o.Name}\t{o.Size}\t{o.OperandType}\t{o.StackBehaviourPop}\t{o.StackBehaviourPush}\t{o.FlowControl}"));

var assembly = args.Length > 1 ? args[1] : typeof(object).Assembly.Location;
var maximum = args.Length > 2 ? int.Parse(args[2]) : 512;
using var file = File.OpenRead(assembly);
using var pe = new PEReader(file);
var metadata = pe.GetMetadataReader();
using var destination = File.Create(Path.Combine(output, "runtime-corpus.bin"));
using var writer = new BinaryWriter(destination);
writer.Write("FNET"u8);
var methods = 0;
var instructions = 0;
var switches = 0;
// A deterministic spread of methods, avoiding an unrepresentative prefix of CoreLib.
foreach (var handle in metadata.MethodDefinitions) {
    var definition = metadata.GetMethodDefinition(handle);
    if (definition.RelativeVirtualAddress == 0) continue;
    var rid = System.Reflection.Metadata.Ecma335.MetadataTokens.GetRowNumber(handle);
    if (rid % 37 != 0) continue;
    var body = pe.GetMethodBody(definition.RelativeVirtualAddress);
    var bytes = body.GetILBytes()!;
    if (bytes.Length == 0) continue;
    var decoded = new List<(int Offset, int Length, ushort Code, ulong Value, uint[] Targets)>();
    var reader = new BinaryReader(new MemoryStream(bytes));
    while (reader.BaseStream.Position < bytes.Length) {
        var start = checked((int)reader.BaseStream.Position);
        ushort code = reader.ReadByte();
        if (code == 0xFE) code = (ushort)(0xFE00 | reader.ReadByte());
        if (!lookup.TryGetValue(code, out var opcode)) throw new InvalidDataException($"Unknown opcode {code:X4}");
        ulong value = 0;
        uint[] targets = [];
        switch (opcode.OperandType) {
            case OperandType.InlineNone: break;
            case OperandType.ShortInlineI:
                value = code == 0xFE12 ? reader.ReadByte() : unchecked((ulong)(long)reader.ReadSByte()); break;
            case OperandType.InlineI: value = unchecked((ulong)(long)reader.ReadInt32()); break;
            case OperandType.InlineI8: value = reader.ReadUInt64(); break;
            case OperandType.ShortInlineR: value = reader.ReadUInt32(); break;
            case OperandType.InlineR: value = reader.ReadUInt64(); break;
            case OperandType.ShortInlineVar: value = reader.ReadByte(); break;
            case OperandType.InlineVar: value = reader.ReadUInt16(); break;
            case OperandType.ShortInlineBrTarget: {
                var relative = reader.ReadSByte(); value = unchecked((ulong)(reader.BaseStream.Position + relative)); break;
            }
            case OperandType.InlineBrTarget: {
                var relative = reader.ReadInt32(); value = unchecked((ulong)(reader.BaseStream.Position + relative)); break;
            }
            case OperandType.InlineSwitch: {
                var count = reader.ReadUInt32();
                var relative = new int[count];
                for (var i = 0; i < count; i++) relative[i] = reader.ReadInt32();
                var tableEnd = checked((int)reader.BaseStream.Position);
                targets = relative.Select(d => unchecked((uint)(tableEnd + d))).ToArray();
                value = count; switches++; break;
            }
            default: value = reader.ReadUInt32(); break;
        }
        decoded.Add((start, checked((int)reader.BaseStream.Position) - start, code, value, targets));
    }
    writer.Write((uint)rid);
    writer.Write((uint)bytes.Length);
    writer.Write(bytes);
    writer.Write((uint)decoded.Count);
    foreach (var instruction in decoded) {
        writer.Write((uint)instruction.Offset);
        writer.Write((uint)instruction.Length);
        writer.Write(instruction.Code);
        writer.Write(instruction.Value);
        foreach (var target in instruction.Targets) writer.Write(target);
    }
    methods++;
    instructions += decoded.Count;
    if (methods >= maximum) break;
}
var summary = $"Runtime: {System.Runtime.InteropServices.RuntimeInformation.FrameworkDescription}\nAssembly: {Path.GetFileName(assembly)}\nMVID: {metadata.GetGuid(metadata.GetModuleDefinition().Mvid)}\nMethods: {methods}\nInstructions: {instructions}\nSwitches: {switches}\n";
File.WriteAllText(Path.Combine(output, "runtime-corpus.txt"), summary);
Console.Write(summary);
