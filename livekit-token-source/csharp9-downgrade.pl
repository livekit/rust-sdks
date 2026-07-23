use strict;
use warnings;

# Downgrade uniffi-bindgen-cs output to compile under C# 9.
#
# Unity is pinned to C# 9 (still true through Unity 6), but uniffi-bindgen-cs emits a
# few C# 10 features. This rewrites the ones its fixed templates can produce, generically
# — no assumptions about a particular crate's API — so any generated *.cs passed as an
# argument becomes C# 9 valid. Idempotent; safe to re-run.
#
# Usage:  perl csharp9-downgrade.pl <file.cs> [<file.cs> ...]
#
# Features handled:
#   (a) file-scoped namespace          `namespace X;`            -> `namespace X { ... }`
#   (b) inferred delegate type         `var f = Conv.INSTANCE.Read;` (method group -> var)
#                                      -> explicit `Func<...>/Action<...>`
#
# If uniffi-bindgen-cs starts emitting another C# 10+ construct, add a pass here.

for my $file (@ARGV) {
    open my $in, '<', $file or die "open $file: $!";
    local $/;
    my $src = <$in>;
    close $in;

    # (a) file-scoped namespace (C# 10) -> block-scoped (C# 9).
    #     Everything after `namespace X;` belongs to the namespace, so wrap to EOF.
    if ($src =~ s/^namespace\s+([\w.]+)\s*;[ \t]*\r?\n/namespace $1\n{\n/m) {
        $src =~ s/\s*\z/\n}\n/;
    }

    # (b) inferred delegate type (C# 10): a method group assigned to `var`, e.g.
    #     `var readerKey = FfiConverterString.INSTANCE.Read;`
    #     uniffi emits these only in collection converters (Dictionary/Sequence), aliasing
    #     the element converter's Read / Write / AllocationSize. C# 9 can't infer the
    #     delegate type from a method group, so give it an explicit Func/Action. The element
    #     type is resolved from the converter's own class declaration, so this works for ANY
    #     element type (primitives, strings, records, optionals, ...), not just this crate.
    my %elem;
    #   primitive converters:  class FfiConverterString: FfiConverter<string, RustBuffer>
    while ($src =~ /class\s+(FfiConverter\w+)\s*:\s*FfiConverter<\s*([^,<>]+?)\s*,\s*RustBuffer\s*>/g) {
        $elem{$1} = $2;
    }
    #   buffer converters:     class FfiConverterTypeFoo: FfiConverterRustBuffer<Foo> {
    #   (non-greedy up to the `>` that is followed by `{`, so nested generics survive)
    while ($src =~ /class\s+(FfiConverter\w+)\s*:\s*FfiConverterRustBuffer<\s*(.+?)\s*>\s*\{/g) {
        $elem{$1} = $2;
    }

    $src =~ s{^([ \t]*)var\s+(\w+)\s*=\s*(FfiConverter\w+)\.INSTANCE\.(Read|AllocationSize|Write)\s*;[ \t]*$}{
        my ($indent, $name, $conv, $method) = ($1, $2, $3, $4);
        my $t = $elem{$conv};
        if (defined $t) {
            my $delegate = $method eq 'Read'           ? "Func<BigEndianStream, $t>"
                         : $method eq 'AllocationSize' ? "Func<$t, int>"
                         :                               "Action<$t, BigEndianStream>";
            "$indent$delegate $name = $conv.INSTANCE.$method;";
        } else {
            # Unknown converter type: leave it so the compiler flags it loudly, rather
            # than us silently emitting a wrong delegate type.
            "${indent}var $name = $conv.INSTANCE.$method;";
        }
    }egm;

    open my $out, '>', $file or die "write $file: $!";
    print $out $src;
    close $out;
}
