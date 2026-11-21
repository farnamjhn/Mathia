"""Pydantic schemas matching math_core Rust data structures and dataset JSONL."""

from __future__ import annotations

from typing import Any, Optional
from pydantic import BaseModel, Field


class Reference(BaseModel):
    kind: str
    number: Optional[str] = None


class KeywordHit(BaseModel):
    kind: str
    number: Optional[str] = None
    name: Optional[str] = None
    labelled: bool = False
    body_empty: bool = False
    reference: bool = False
    refers_to: Optional[Reference] = None


class NodeFeatures(BaseModel):
    lead: Optional[KeywordHit] = None
    mentions: list[str] = Field(default_factory=list)
    emphasized: bool = False
    math_density: float = 0.0
    logic_count: int = 0
    logic_density: float = 0.0
    qed: bool = False
    definitional: bool = False
    proof_cue: bool = False
    statement_cue: bool = False
    continuation: bool = False


class EnvSpan(BaseModel):
    kind: str
    name: Optional[str] = None
    len: int


class DumpNode(BaseModel):
    index: int
    kind: str
    flat_text: str
    char_len: int
    env_span: Optional[EnvSpan] = None


class FeatureDump(BaseModel):
    nodes: list[DumpNode]
    features: list[NodeFeatures]
    bio_tags: Optional[list[str]] = None


class LabeledDocument(BaseModel):
    doc_id: str
    language: str  # "en" or "fa"
    title: str = ""
    nodes: list[DumpNode]
    features: list[NodeFeatures]
    bio_tags: list[str]
    metadata: dict[str, Any] = Field(default_factory=dict)
