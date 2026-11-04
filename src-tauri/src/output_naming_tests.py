"""Filesystem failure/coexistence contracts; no ComfyUI dependencies or listener."""
import errno
import os
from pathlib import Path
import sys
import tempfile
from types import ModuleType, SimpleNamespace
import unittest
from unittest.mock import patch

sys.modules['folder_paths'] = ModuleType('folder_paths')
nodes = ModuleType('nodes')
setattr(nodes, 'NODE_CLASS_MAPPINGS', {})
sys.modules['nodes'] = nodes
server = ModuleType('server')
setattr(server, 'PromptServer', SimpleNamespace(instance=SimpleNamespace(add_on_prompt_handler=lambda handler: None)))
sys.modules['server'] = server
adapter = ModuleType('deas_output_naming_test')
exec(compile(sys.argv.pop(1), 'output_naming.py', 'exec'), adapter.__dict__)


class PublicationTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.sources = [self.root / name for name in ('clip_00001_audio.webm', 'clip_00001_audio-first-frame.png')]
        for i, source in enumerate(self.sources):
            source.write_bytes(bytes([i]))
        self.files = [(source, adapter.SAVED.fullmatch(source.name)) for source in self.sources]

    def test_second_link_failure_rolls_back_and_preserves_original_error(self):
        link = os.link
        failure = OSError(errno.EIO, 'second link failed')
        calls = []

        def fail_second(source, target):
            calls.append(target)
            if len(calls) == 2:
                raise failure
            link(source, target)

        unrelated = self.root / 'other.webm'
        unrelated.write_bytes(b'untouched')
        with patch.object(adapter.os, 'link', side_effect=fail_second):
            with self.assertRaises(OSError) as raised:
                adapter.publish(self.files)
        self.assertIs(raised.exception, failure)
        self.assertEqual(len(calls), 2, 'non-collision failures must not retry')
        self.assertEqual(set(self.root.iterdir()), {*self.sources, unrelated})
        self.assertEqual([p.read_bytes() for p in self.sources], [b'\0', b'\1'])
        self.assertEqual(unrelated.read_bytes(), b'untouched')

    def test_second_link_collision_retries_whole_group_without_clobber(self):
        occupied = self.root / 'clip-00001-audio-first-frame.png'
        occupied.write_bytes(b'foreign')
        renamed = adapter.publish(self.files)
        self.assertEqual(occupied.read_bytes(), b'foreign')
        self.assertFalse((self.root / 'clip-00001-audio.webm').exists())
        self.assertEqual(list(renamed.values()), ['clip-00002-audio.webm', 'clip-00002-audio-first-frame.png'])
        self.assertEqual([(self.root / name).read_bytes() for name in renamed.values()], [b'\0', b'\1'])
        self.assertTrue(all(not source.exists() for source in self.sources))

    def test_interrupt_also_rolls_back(self):
        link = os.link
        failure = KeyboardInterrupt()
        calls = []

        def interrupt_second(source, target):
            calls.append(target)
            if len(calls) == 2:
                raise failure
            link(source, target)

        with patch.object(adapter.os, 'link', side_effect=interrupt_second):
            with self.assertRaises(KeyboardInterrupt) as raised:
                adapter.publish(self.files)
        self.assertIs(raised.exception, failure)
        self.assertEqual(set(self.root.iterdir()), set(self.sources))

    def test_rollback_failure_keeps_original_error_and_cleans_remaining_links(self):
        third = self.root / 'clip_00001_audio-last-frame.png'
        third.write_bytes(b'third')
        files = self.files + [(third, adapter.SAVED.fullmatch(third.name))]
        link, unlink = os.link, Path.unlink
        calls, cleanup = [], []
        failure = FileExistsError(errno.EEXIST, 'third target occupied')

        def fail_third(source, target):
            calls.append(target)
            if len(calls) == 3:
                raise failure
            link(source, target)

        def deny_first(target):
            cleanup.append(target)
            if len(cleanup) == 1:
                raise PermissionError(errno.EACCES, 'rollback denied')
            unlink(target)

        with patch.object(adapter.os, 'link', side_effect=fail_third), patch.object(Path, 'unlink', deny_first):
            with self.assertRaises(FileExistsError) as raised:
                adapter.publish(files)
        self.assertIs(raised.exception, failure)
        self.assertIn('rollback denied', str(failure.__notes__))
        self.assertEqual(len(calls), 3, 'failed rollback must prevent collision retry')
        self.assertEqual(len(cleanup), 2, 'cleanup must continue after an unlink failure')
        self.assertTrue(all(source.is_file() for source, _ in files))
        self.assertTrue(calls[0].is_file())
        self.assertFalse(calls[1].exists())

    def test_first_link_permission_failure_preserves_sources(self):
        with patch.object(adapter.os, 'link', side_effect=PermissionError(errno.EACCES, 'denied')) as link:
            with self.assertRaises(PermissionError):
                adapter.publish(self.files)
        self.assertEqual(link.call_count, 1)
        self.assertEqual(set(self.root.iterdir()), set(self.sources))


class CapabilityTests(unittest.TestCase):
    def test_unsupported_savers_do_not_break_prompt_or_other_savers(self):
        request = {'extra_data': {'extra_pnginfo': {'deas_output_naming': True}}}
        class Supported:
            FUNCTION = 'save'
            def save(self, **kwargs):
                return {'ui': {}}
        for unsupported in (type('MissingFunction', (), {}), type('MissingMethod', (), {'FUNCTION': 'missing'}),
                            type('InvalidFunction', (), {'FUNCTION': None}), type('NotCallable', (), {'FUNCTION': 'save', 'save': 1})):
            with self.subTest(unsupported=unsupported.__name__), patch.dict(nodes.NODE_CLASS_MAPPINGS,
                    {'SaveImage': unsupported, 'DaSiWa_MetadataImageSaver': Supported}, clear=True):
                self.assertIs(adapter.prepare(request), request)
                wrapped = Supported.save
                self.assertTrue(getattr(wrapped, '_deas_naming', False))
                self.assertIs(adapter.prepare(request), request)
                self.assertIs(Supported.save, wrapped)
                self.assertEqual(Supported().save(extra_pnginfo={'deas_output_naming': True}), {'ui': {}})

    def test_unmarked_prompt_does_not_wrap(self):
        class Saver:
            FUNCTION = 'save'
            def save(self):
                return None
        original = Saver.save
        with patch.dict(nodes.NODE_CLASS_MAPPINGS, {'SaveImage': Saver}, clear=True):
            self.assertEqual(adapter.prepare({}), {})
        self.assertIs(Saver.save, original)


unittest.main()
