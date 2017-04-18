# show.py - Extension implementing `hg show`
#
# Copyright 2017 Gregory Szorc <gregory.szorc@gmail.com>
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2 or any later version.

"""unified command to show various repository information (EXPERIMENTAL)

This extension provides the :hg:`show` command, which provides a central
command for displaying commonly-accessed repository data and views of that
data.
"""

from __future__ import absolute_import

from mercurial.i18n import _
from mercurial.node import nullrev
from mercurial import (
    cmdutil,
    error,
    formatter,
    graphmod,
    pycompat,
    registrar,
    revset,
    revsetlang,
)

# Note for extension authors: ONLY specify testedwith = 'ships-with-hg-core' for
# extensions which SHIP WITH MERCURIAL. Non-mainline extensions should
# be specifying the version(s) of Mercurial they are tested with, or
# leave the attribute unspecified.
testedwith = 'ships-with-hg-core'

cmdtable = {}
command = cmdutil.command(cmdtable)
revsetpredicate = registrar.revsetpredicate()

class showcmdfunc(registrar._funcregistrarbase):
    """Register a function to be invoked for an `hg show <thing>`."""

    # Used by _formatdoc().
    _docformat = '%s -- %s'

    def _extrasetup(self, name, func, fmtopic=None):
        """Called with decorator arguments to register a show view.

        ``name`` is the sub-command name.

        ``func`` is the function being decorated.

        ``fmtopic`` is the topic in the style that will be rendered for
        this view.
        """
        func._fmtopic = fmtopic

showview = showcmdfunc()

@command('show', [
    # TODO: Switch this template flag to use commands.formatteropts if
    # 'hg show' becomes stable before --template/-T is stable. For now,
    # we are putting it here without the '(EXPERIMENTAL)' flag because it
    # is an important part of the 'hg show' user experience and the entire
    # 'hg show' experience is experimental.
    ('T', 'template', '', ('display with template'), _('TEMPLATE')),
    ], _('VIEW'))
def show(ui, repo, view=None, template=None):
    """show various repository information

    A requested view of repository data is displayed.

    If no view is requested, the list of available views is shown and the
    command aborts.

    .. note::

       There are no backwards compatibility guarantees for the output of this
       command. Output may change in any future Mercurial release.

       Consumers wanting stable command output should specify a template via
       ``-T/--template``.

    List of available views:
    """
    if ui.plain() and not template:
        hint = _('invoke with -T/--template to control output format')
        raise error.Abort(_('must specify a template in plain mode'), hint=hint)

    views = showview._table

    if not view:
        ui.pager('show')
        # TODO consider using formatter here so available views can be
        # rendered to custom format.
        ui.write(_('available views:\n'))
        ui.write('\n')

        for name, func in sorted(views.items()):
            ui.write(('%s\n') % func.__doc__)

        ui.write('\n')
        raise error.Abort(_('no view requested'),
                          hint=_('use "hg show VIEW" to choose a view'))

    # TODO use same logic as dispatch to perform prefix matching.
    if view not in views:
        raise error.Abort(_('unknown view: %s') % view,
                          hint=_('run "hg show" to see available views'))

    template = template or 'show'
    fmtopic = 'show%s' % views[view]._fmtopic

    ui.pager('show')
    with ui.formatter(fmtopic, {'template': template}) as fm:
        return views[view](ui, repo, fm)

@showview('bookmarks', fmtopic='bookmarks')
def showbookmarks(ui, repo, fm):
    """bookmarks and their associated changeset"""
    marks = repo._bookmarks
    if not len(marks):
        # This is a bit hacky. Ideally, templates would have a way to
        # specify an empty output, but we shouldn't corrupt JSON while
        # waiting for this functionality.
        if not isinstance(fm, formatter.jsonformatter):
            ui.write(_('(no bookmarks set)\n'))
        return

    active = repo._activebookmark
    longestname = max(len(b) for b in marks)
    # TODO consider exposing longest shortest(node).

    for bm, node in sorted(marks.items()):
        fm.startitem()
        fm.context(ctx=repo[node])
        fm.write('bookmark', '%s', bm)
        fm.write('node', fm.hexfunc(node), fm.hexfunc(node))
        fm.data(active=bm == active,
                longestbookmarklen=longestname)

@revsetpredicate('_underway([commitage[, headage]])')
def underwayrevset(repo, subset, x):
    args = revset.getargsdict(x, 'underway', 'commitage headage')
    if 'commitage' not in args:
        args['commitage'] = None
    if 'headage' not in args:
        args['headage'] = None

    # We assume callers of this revset add a topographical sort on the
    # result. This means there is no benefit to making the revset lazy
    # since the topographical sort needs to consume all revs.
    #
    # With this in mind, we build up the set manually instead of constructing
    # a complex revset. This enables faster execution.

    # Mutable changesets (non-public) are the most important changesets
    # to return. ``not public()`` will also pull in obsolete changesets if
    # there is a non-obsolete changeset with obsolete ancestors. This is
    # why we exclude obsolete changesets from this query.
    rs = 'not public() and not obsolete()'
    rsargs = []
    if args['commitage']:
        rs += ' and date(%s)'
        rsargs.append(revsetlang.getstring(args['commitage'],
                                           _('commitage requires a string')))

    mutable = repo.revs(rs, *rsargs)
    relevant = revset.baseset(mutable)

    # Add parents of mutable changesets to provide context.
    relevant += repo.revs('parents(%ld)', mutable)

    # We also pull in (public) heads if they a) aren't closing a branch
    # b) are recent.
    rs = 'head() and not closed()'
    rsargs = []
    if args['headage']:
        rs += ' and date(%s)'
        rsargs.append(revsetlang.getstring(args['headage'],
                                           _('headage requires a string')))

    relevant += repo.revs(rs, *rsargs)

    # Add working directory parent.
    wdirrev = repo['.'].rev()
    if wdirrev != nullrev:
        relevant += revset.baseset(set([wdirrev]))

    return subset & relevant

@showview('underway', fmtopic='underway')
def showunderway(ui, repo, fm):
    """changesets that aren't finished"""
    # TODO support date-based limiting when calling revset.
    revs = repo.revs('sort(_underway(), topo)')

    revdag = graphmod.dagwalker(repo, revs)
    displayer = cmdutil.changeset_templater(ui, repo, None, None,
                                            tmpl=fm._t.load(fm._topic),
                                            mapfile=None, buffered=True)

    ui.setconfig('experimental', 'graphshorten', True)
    cmdutil.displaygraph(ui, repo, revdag, displayer, graphmod.asciiedges)

# Adjust the docstring of the show command so it shows all registered views.
# This is a bit hacky because it runs at the end of module load. When moved
# into core or when another extension wants to provide a view, we'll need
# to do this more robustly.
# TODO make this more robust.
def _updatedocstring():
    longest = max(map(len, showview._table.keys()))
    entries = []
    for key in sorted(showview._table.keys()):
        entries.append(pycompat.sysstr('    %s   %s' % (
            key.ljust(longest), showview._table[key]._origdoc)))

    cmdtable['show'][0].__doc__ = pycompat.sysstr('%s\n\n%s\n    ') % (
        cmdtable['show'][0].__doc__.rstrip(),
        pycompat.sysstr('\n\n').join(entries))

_updatedocstring()
