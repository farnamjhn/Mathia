from mathia_ml.evaluate import (
    compute_metrics,
    evaluate_dataset,
    format_report_markdown,
    tag_heuristic,
)
from mathia_ml.export_feedback import (
    export_feedback_directory,
    export_feedback_from_markdown_and_overrides,
    export_feedback_telemetry,
    load_overrides_dict,
)
from mathia_ml.features import (
    FEATURE_SPEC_VERSION,
    WINDOW_OFFSETS,
    bucket_logic_density,
    bucket_math_density,
    extract_dataset,
    extract_document_features,
    extract_node_base_features,
    extract_node_features,
    extract_sequence_features,
    get_math_density_delta,
    node_ends_with_colon,
    offset_prefix,
    summarize_features,
)
from mathia_ml.schema import DumpNode, FeatureDump, LabeledDocument, NodeFeatures
from mathia_ml.split import (
    compute_dataset_stats,
    load_jsonl,
    split_documents,
    verify_split_directory,
    write_split_jsonl,
)
from mathia_ml.train_crf import (
    document_source,
    export_crf_weights_json,
    format_crf_weights,
    oversample_correction_documents,
    train_and_export,
    train_crf_model,
    tune_hyperparameters,
    tune_transition_temperature,
    verify_viterbi_parity,
    viterbi_decode,
)

__all__ = [
    "DumpNode",
    "FeatureDump",
    "LabeledDocument",
    "NodeFeatures",
    "FEATURE_SPEC_VERSION",
    "WINDOW_OFFSETS",
    "offset_prefix",
    "bucket_math_density",
    "bucket_logic_density",
    "get_math_density_delta",
    "node_ends_with_colon",
    "extract_node_base_features",
    "extract_node_features",
    "extract_sequence_features",
    "extract_document_features",
    "extract_dataset",
    "summarize_features",
    "train_crf_model",
    "tune_hyperparameters",
    "tune_transition_temperature",
    "oversample_correction_documents",
    "document_source",
    "format_crf_weights",
    "export_crf_weights_json",
    "viterbi_decode",
    "verify_viterbi_parity",
    "train_and_export",
    "split_documents",
    "write_split_jsonl",
    "load_jsonl",
    "compute_dataset_stats",
    "verify_split_directory",
    "export_feedback_from_markdown_and_overrides",
    "export_feedback_directory",
    "export_feedback_telemetry",
    "load_overrides_dict",
    "tag_heuristic",
    "compute_metrics",
    "evaluate_dataset",
    "format_report_markdown",
]


def main() -> None:
    print("Mathia ML Toolkit")
