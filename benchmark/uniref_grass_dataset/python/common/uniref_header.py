"""Parsing of UniRef50 FASTA header lines.

UniRef50's FASTA release contains exactly one sequence per cluster: the cluster's
representative. Its header already carries everything this pipeline needs (cluster id,
representative description, member count, taxonomy) without requiring the heavier UniRef
XML release. Format:

    >UniRef50_<cluster_id> <description> n=<n_members> Tax=<tax_name> TaxID=<taxid> RepID=<repid>

`<cluster_id>` (the part after "UniRef50_") is the representative's UniProtKB accession in
the vast majority of cases. The exception: clusters whose representative sequence only
exists in UniParc (not UniProtKB, e.g. obsolete/merged entries) use a `UPI...` UniParc
identifier there instead of an accession. Since this pipeline's baseline is UniProtKB
(Swissprot union trEMBL), such clusters must be excluded during sampling.
"""

import re
from dataclasses import dataclass
from typing import Optional

HEADER_RE = re.compile(
    r"^>(?P<cluster_id>\S+)\s+(?P<description>.+?)\s+n=(?P<n_members>\d+)"
    r"(?:\s+Tax=(?P<tax>.+?))?"
    r"(?:\s+TaxID=(?P<taxid>\d+))?"
    r"(?:\s+RepID=(?P<repid>\S+))?"
    r"\s*$"
)

UNIPARC_PREFIX = "UPI"


class UnparsableHeaderError(ValueError):
    """Raised when a UniRef50 FASTA header line doesn't match the expected format."""


@dataclass(frozen=True)
class UniRefHeader:
    cluster_id: str
    description: str
    n_members: int
    tax_name: Optional[str]
    taxid: Optional[int]
    repid: Optional[str]

    @property
    def representative_accession(self) -> str:
        """The representative's UniProtKB accession (or UniParc UPI id), i.e. `cluster_id`
        with the `UniRef50_` prefix stripped."""
        return self.cluster_id.split("_", 1)[1] if "_" in self.cluster_id else self.cluster_id

    @property
    def is_uniparc_only(self) -> bool:
        """True if the representative sequence has no UniProtKB accession (UniParc-only)."""
        return self.representative_accession.startswith(UNIPARC_PREFIX)


def parse_header(line: str) -> UniRefHeader:
    """Parses a single UniRef50 FASTA header line (including the leading '>').

    Raises `UnparsableHeaderError` if the line doesn't match the expected format.
    """
    match = HEADER_RE.match(line.strip())
    if not match:
        raise UnparsableHeaderError(f"Could not parse UniRef50 header line: {line!r}")
    groups = match.groupdict()
    return UniRefHeader(
        cluster_id=groups["cluster_id"],
        description=groups["description"],
        n_members=int(groups["n_members"]),
        tax_name=groups["tax"],
        taxid=int(groups["taxid"]) if groups["taxid"] is not None else None,
        repid=groups["repid"],
    )
